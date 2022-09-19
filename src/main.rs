use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use processor::scan_music;
use serenity::async_trait;
use serenity::http::CacheHttp;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;

mod processor;

struct Handler;

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

            for attachment in msg.attachments {
                if let Some(content_type) = attachment.content_type {
                    match content_type.as_str() {
                        "image/png" | "image/jpeg" | "image/webp" | "image/bmp" | "image/gif" => {}
                        _ => {
                            if let Err(why) =
                                msg.channel_id.say(&ctx.http, "Unsupported file type").await
                            {
                                println!("Error sending message: {:?}", why);
                            }
                            continue;
                        }
                    }
                }
            }

            if let Err(why) = msg.channel_id.say(&ctx.http, "What?").await {
                println!("Error sending message: {:?}", why);
            };
        }
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, _: Context, ready: Ready) {
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
    println!("Found {} suitable tracks", music.len(),);

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
    type Value = Arc<RwLock<HashMap<PathBuf, f32>>>;
}
