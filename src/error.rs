use core::fmt;
use std::io;

use serenity::{
    builder::CreateMessage,
    http::Http,
    model::channel::{Message, MessageReference},
    prelude::*,
    utils::colours,
};

use crate::spotify::{SpotifyError};

#[derive(Debug)]
pub enum MiitopiaError {
    Serenity(SerenityError),
    Ffmpeg(String),
    Io(io::Error),
    InvalidFileType,
    UnsupportedFileType(String),
    Reqwest(reqwest::Error),
    NoTracks,
    Spotify(SpotifyError)
}

impl fmt::Display for MiitopiaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MiitopiaError::Serenity(e) => write!(f, "Serenity Error: {}", e),
            MiitopiaError::Ffmpeg(e) => write!(f, "Ffmpeg Error: {}", e),
            MiitopiaError::Io(e) => write!(f, "IO Error: {}", e),
            MiitopiaError::InvalidFileType => write!(f, "Invalid File Type"),
            MiitopiaError::UnsupportedFileType(mime) => {
                write!(f, "Unsupported File Type: {}", mime)
            },
            MiitopiaError::NoTracks => write!(f, "No Tracks"),
            MiitopiaError::Reqwest(e) => write!(f, "Reqwest Error: {}", e),
            MiitopiaError::Spotify(e) => write!(f, "Spotify API Error: {}", e),
        }
    }
}

impl From<io::Error> for MiitopiaError {
    fn from(e: io::Error) -> Self {
        MiitopiaError::Io(e)
    }
}
impl From<SerenityError> for MiitopiaError {
    fn from(e: SerenityError) -> Self {
        MiitopiaError::Serenity(e)
    }
}

impl From<reqwest::Error> for MiitopiaError {
    fn from(e: reqwest::Error) -> Self {
        MiitopiaError::Reqwest(e)
    }
}

impl From<SpotifyError> for MiitopiaError {
    fn from(e: SpotifyError) -> Self {
        MiitopiaError::Spotify(e)
    }
}

impl MiitopiaError {
    pub fn embed_error(&self, msg: &mut CreateMessage) {
        msg.add_embed(|em| {
            match self {
                MiitopiaError::Serenity(e) => em.title("🔥 Serenity Error").description(e),
                MiitopiaError::Ffmpeg(e) => em.title("🎞 FFmpeg Error").description(e),
                MiitopiaError::Io(e) => em.title("💾 IO Error").description(e),
                MiitopiaError::InvalidFileType => em.title("🚩Invalid File Type"),
                MiitopiaError::UnsupportedFileType(mime) => em
                    .title("🤷 Unsupported File")
                    .description(format!("The file type *{}* is not supported.", mime)),
                MiitopiaError::Reqwest(e) => em.title("🌐 Requwest ˘꒳˘ Error ").description(e),
                MiitopiaError::NoTracks => em.title("🔥 No Audio Found").description("Miitopia could not find any audio."),
                MiitopiaError::Spotify(e) => em.title(format!("🌐 Spotify Error")).description(e),
                
            };
            em.color(colours::css::DANGER).footer(|f| {
                f.text("If you think this is a mistake, report this issue on github. https://github.com/SeeSharpeDen/miitopia/issues").icon_url("https://github.com/SeeSharpeDen/miitopia/raw/master/resources/discord-profile.png")
            })
        });
    }
    pub async fn reply_error(
        &self,
        http: impl AsRef<Http>,
        reference: MessageReference,
    ) -> Result<Message, SerenityError> {
        reference
            .channel_id
            .send_message(http, |msg| {
                self.embed_error(msg);
                msg.reference_message(reference)
            })
            .await
    }
}