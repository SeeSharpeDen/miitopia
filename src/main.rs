use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use indexmap::IndexMap;
use processor::{apply_music, get_rand_track, scan_music};
use rand::prelude::SmallRng;
use rand::{Rng, SeedableRng};
use serenity::http::CacheHttp;
use serenity::model::channel::{AttachmentType, Message};
use serenity::model::gateway::Ready;
use serenity::utils::colours;
use serenity::{async_trait, futures, prelude::*};

mod error;
mod processor;

struct Handler;

const MAX_LENGTH: f32 = 10.0;

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.mentions_me(ctx.http()).await.unwrap_or_default() {
            // Broadcast that we are typing.
            let _ = ctx.http().broadcast_typing(msg.channel_id.0);

            // get our tracks.
            let data_read = ctx.data.read().await;
            let tracks = data_read
                .get::<Music>()
                .expect("Expected Music in TypeMap")
                .read()
                .await;

            let mut rng = SmallRng::from_entropy();

            // Start processing the attachments.
            let mut raw_futures = Vec::new();
            for attachment in msg.attachments {
                // Get a random track and duration.
                let (track, track_duration) =
                    get_rand_track(&tracks, &mut rng).expect("No track found");

                // start at zero and clip the duration to our MAX.
                let mut start: f32 = 0.0;
                let duration = track_duration.min(MAX_LENGTH);

                // only update start if the track is longer than our max.
                if track_duration > MAX_LENGTH {
                    start = rng.gen_range(0.0..track_duration - duration);
                }
                raw_futures.push(apply_music(track, start, duration, attachment));
            }
            let unpin_futures: Vec<_> = raw_futures.into_iter().map(Box::pin).collect();
            let mut futures = unpin_futures;

            while !futures.is_empty() {
                match futures::future::select_all(futures).await {
                    (Ok(file_bytes), _index, remaining) => {
                        futures = remaining;
                        if let Err(why) = msg
                            .channel_id
                            .send_message(&ctx.http, |m| {
                                m.add_file(AttachmentType::Bytes {
                                    data: file_bytes.into(),
                                    filename: "miitopia.webm".to_string(),
                                })
                            })
                            .await
                        {
                            println!("Error sending message: {:?}", why);
                        }
                    }
                    (Err(error), _index, remaining) => {
                        // Update the futures.
                        futures = remaining;

                        // Do something about the error.
                        println!("Error: {:?}", error);

                        // Create an error.
                        if let Err(why) = msg
                            .channel_id
                            .send_message(&ctx.http, |m| {
                                m.add_embed(|em| {
                                    em.description(error.to_string())
                                        .colour(colours::css::DANGER)
                                        .title("⚠️ Error")
                                })
                            })
                            .await
                        {
                            println!("Error sending message: {:?}", why);
                        }
                    }
                }
            }
        }
    }
    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    // Scan all our music
    println!("Scanning /resources/music");
    let music = scan_music();
    println!("Found {} tracks", music.len(),);

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<Music>(Arc::new(RwLock::new(music)));
    }

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}

struct Music;

impl TypeMapKey for Music {
    type Value = Arc<RwLock<IndexMap<PathBuf, f32>>>;
}
