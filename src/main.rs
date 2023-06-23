use std::borrow::Cow;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use audio_source::AudioSource;
use human_repr::{HumanCount, HumanDuration};
use indexmap::IndexMap;
use log::{debug, error, info, trace, warn};
use processor::{apply_music, scan_music};
use rand::prelude::SmallRng;
use rand::SeedableRng;
use regex::Regex;
use serenity::http::CacheHttp;
use serenity::model::channel::{AttachmentType, Message, MessageReference};
use serenity::model::gateway::Ready;
use serenity::model::prelude::Reaction;
use serenity::{async_trait, futures, prelude::*};

mod audio_source;
mod error;
mod processor;
mod spotify;

const MAX_AUDIO_LENGTH: f32 = 10.0;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        // Bail out if the message doesn't mention this bot.
        if !msg.mentions_me(ctx.http()).await.unwrap_or_default() {
            return;
        }

        match processor::process_message(&ctx, &msg).await {
            Ok(()) => {
                // Good!
            }
            Err(reasons) => {
                for reason in reasons {
                    let r = MessageReference::from((msg.channel_id, msg.id));
                    if let Err(why) = reason.reply_error(&ctx.http, r).await {
                        warn!("Failed to send error message: {:?}", why);
                    }
                }
            }
        }
    }
    // In this case, just print what the current user's username is.
    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }

    async fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        // Get the reaction if it's a custom emoji.
        // let msg = reaction.message(ctx.http).await;
        if let serenity::model::prelude::ReactionType::Custom {
            animated: _,
            id,
            name,
        } = reaction.emoji.clone()
        {
            // Only do something if the name of the emoji is "miitopia"
            match name {
                Some(name) if name == "miitopia" => {
                    info!("Someone used the {name} emoji (emoji id: {id})");
                    let msg = reaction.message(ctx.http.clone()).await;
                    match msg {
                        Ok(msg) => match processor::process_message(&ctx, &msg).await {
                            Ok(()) => {
                                // Good!
                            }
                            Err(reasons) => {
                                for reason in reasons {
                                    let r = MessageReference::from((msg.channel_id, msg.id));
                                    if let Err(why) = reason.reply_error(&ctx.http, r).await {
                                        warn!("Failed to send error message: {:?}", why);
                                    }
                                }
                            }
                        },
                        Err(reason) => {
                            log::error!("Failed to get the message. Reason: {reason}");
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MESSAGE_REACTIONS;

    // Get a spotify token.
    let client_id = env::var("SPOTIFY_ID");
    let spotify = match client_id {
        Ok(client_id) => {
            let client_secret = env::var("SPOTIFY_SECRET")
                .expect("If SPOTIFY_ID is provided SPOTIFY_SECRET is required");

            match spotify::Spotify::from_credentials(client_id, client_secret).await {
                Ok(spotify) => Some(spotify),
                Err(e) => {
                    panic!("Spotify Error: {}", e);
                }
            }
        }
        Err(_) => None,
    };

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
        // Add our music and spotify to the context data.
        let mut data = client.data.write().await;
        data.insert::<Music>(Arc::new(RwLock::new(music)));
        if let Some(spotify) = spotify {
            data.insert::<spotify::Spotify>(Arc::new(RwLock::new(spotify)));
        }
    }

    // Finally, start a single shard, and start listening to events.
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
