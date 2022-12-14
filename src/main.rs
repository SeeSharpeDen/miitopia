use std::borrow::Cow;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use audio_source::AudioSource;
use human_repr::{HumanCount, HumanDuration};
use indexmap::IndexMap;
use log::{debug, error, info, warn};
use processor::{apply_music, scan_music};
use rand::prelude::SmallRng;
use rand::SeedableRng;
use regex::Regex;
use serenity::http::CacheHttp;
use serenity::model::channel::{AttachmentType, Message, MessageReference};
use serenity::model::gateway::Ready;
use serenity::{async_trait, futures, prelude::*};

mod audio_source;
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

            debug!("{}: {}", msg.author.name, msg.content_safe(&ctx.cache));

            // get our tracks.
            let data_read = ctx.data.read().await;
            let tracks = data_read
                .get::<Music>()
                .expect("Expected Music in TypeMap")
                .read()
                .await;

            // Setup our rng.
            let mut rng = SmallRng::from_entropy();

            let mut source = AudioSource::Miitopia;

            // If the message contains a url assume it's an audio source.
            let re = Regex::new(r"https://[^\s]*").unwrap();
            if let Some(url) = re.captures_iter(&msg.content_safe(&ctx.cache)).next() {
                source = AudioSource::Url(url[0].to_string());
            }

            // If we cannot validate the source, return the error.
            if let Some(error) = source.validate(&tracks).await {
                let r = MessageReference::from((msg.channel_id, msg.id)).clone();
                if let Err(why) = error.reply_error(&ctx.http, r).await {
                    warn!("Failed to send error message: {:?}", why);
                }
                return;
            }

            // Start processing the attachments.
            let mut raw_futures = Vec::new();
            for attachment in msg.attachments {
                let track = source.get_track(&tracks, &mut rng);
                match track {
                    Ok((path, start)) => {
                        raw_futures.push(apply_music(path, start, MAX_LENGTH, attachment))
                    }
                    Err(_) => todo!(),
                }
            }
            let unpin_futures: Vec<_> = raw_futures.into_iter().map(Box::pin).collect();
            let mut futures = unpin_futures;

            while !futures.is_empty() {
                match futures::future::select_all(futures).await {
                    (Ok(job), _index, remaining) => {
                        futures = remaining;

                        // TODO: Don't print this (clone stderr!!) if env_logger isn't logging info.
                        info!(
                            "Processed {}\n\tSize: {}\n\tTime: {}\n\tTrack: {}\n\tffmpeg stderr: {}",
                            job.attachment.url,
                            job.output_file.len().human_count_bytes(),
                            job.job_time.human_duration(),
                            job.audio_file,
                            job.stderr.clone().unwrap_or("empty".to_string())
                        );
                        if let Err(why) = msg
                            .channel_id
                            .send_message(&ctx.http, |m| {
                                m.add_file(AttachmentType::Bytes {
                                    data: Cow::from(job.output_file),
                                    filename: "miitopia.webm".to_string(),
                                })
                            })
                            .await
                        {
                            warn!("Error sending message: {:?}", why);
                        }
                    }
                    (Err(error), _index, remaining) => {
                        // Update the futures.
                        futures = remaining;

                        // Print the error to the console.
                        error!("Error: {}", error);

                        // Reply with an error message.
                        let r = MessageReference::from((msg.channel_id, msg.id)).clone();
                        if let Err(why) = error.reply_error(&ctx.http, r).await {
                            warn!("Failed to send error message: {:?}", why);
                        }
                    }
                }
            }
        }
    }
    // In this case, just print what the current user's username is.
    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    // Scan all our music
    info!("Scanning /resources/music");
    let music = scan_music();
    if music.len() > 0 {
        info!("Found {} tracks", music.len(),);
    } else {
        error!("no tracks found.");
    }

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
        error!("Client error: {:?}", why);
    }
}

struct Music;

impl TypeMapKey for Music {
    type Value = Arc<RwLock<IndexMap<PathBuf, f32>>>;
}
