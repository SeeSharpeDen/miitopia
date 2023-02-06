use std::sync::Arc;
use std::{fmt, path::PathBuf};

use log::{debug, trace, warn};
use rand::{prelude::SmallRng, Rng};
use regex::Regex;
use reqwest::header::CONTENT_TYPE;
use serde_json::Value;
use serenity::prelude::*;

use crate::spotify::SpotifyError;
use crate::Music;
use crate::{error::MiitopiaError, spotify::Spotify, MAX_LENGTH};

pub enum AudioSource {
    Miitopia,
    Url(String),
    Spotify(String),
}

impl fmt::Display for AudioSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioSource::Miitopia => write!(f, "Miitopia"),
            AudioSource::Url(url) => write!(f, "Url:{}", url),
            AudioSource::Spotify(id) => write!(f, "Spotify track:{}", id),
        }
    }
}

impl AudioSource {
    pub fn from_msg_content(msg_content: &String) -> AudioSource {
        // Check for spotify matches.
        let spotify_re = Regex::new(r"https://open.spotify.com/track/([a-zA-Z0-9]*)").unwrap();
        if let Some(captures) = spotify_re.captures(msg_content) {
            if let Some(id) = captures.get(1) {
                return AudioSource::Spotify(id.as_str().to_string());
            }
        }

        // Check fo regular http matches.
        let https_re = Regex::new(r"https://[^\s]*").unwrap();
        if let Some(captures) = https_re.captures(msg_content) {
            if let Some(id) = captures.get(0) {
                return AudioSource::Url(id.as_str().to_string());
            }
        }

        // Not url or spotify... must be miitopia.
        return AudioSource::Miitopia;
    }

    pub async fn get_track(
        &self,
        ctx_data: &Arc<RwLock<TypeMap>>,
        rng: &mut SmallRng,
    ) -> Result<(String, f32), MiitopiaError> {
        match self {
            AudioSource::Miitopia => {
                // Get our music from the data_read lock.
                let data_read = ctx_data.read().await;
                let tracks = data_read
                    .get::<Music>()
                    .expect("Expected Music in TypeMap")
                    .read()
                    .await;

                // Get a random track.
                let index = rng.gen_range(0..tracks.len());
                if let Some((path, track_duration)) = tracks.get_index(index) {
                    // If the track is longer than MAX_LENGTH, return track with a random start time.
                    let start_max = track_duration - track_duration.min(MAX_LENGTH);
                    let mut start = 0.0;
                    if start_max > 0.0 {
                        start = rng.gen_range(0.0..start_max);
                    }
                    trace!("Using {} starting at {} seconds", path.display(), start);
                    return Ok((
                        path.to_owned().into_os_string().into_string().unwrap(),
                        start,
                    ));
                }
                return Err(MiitopiaError::NoTracks);
            }
            AudioSource::Url(url) => {
                let result = reqwest::get(url).await?;
                trace!("Downloading \"{}\" to get mimetype.", url);
                if let Ok(mime) = result.headers()[CONTENT_TYPE].to_str() {
                    match mime {
                        // Return the url if it's supported.
                        "audio/mpeg" | "audio/ogg" | "audio/vorbis" => {
                            return Ok((url.to_string(), 0.0))
                        }
                        content_type => {
                            return Err(MiitopiaError::UnsupportedFileType(
                                content_type.to_string(),
                            ))
                        }
                    }
                }
                return Err(MiitopiaError::UnsupportedFileType("Unknown".to_string()));
            }
            AudioSource::Spotify(id) => {
                // Get our music from the data_read lock.
                trace!("Getting instance of spotify.");
                let data_read = ctx_data.read().await;
                let spotify = data_read
                    .get::<Spotify>()
                    .expect("Expected Spotify in TypeMap")
                    .read()
                    .await;
                trace!("Got an instance of spotify.");
                let json = spotify
                    .clone()
                    // TODO: move market into a variable a user can change.
                    .get(format!("https://api.spotify.com/v1/tracks/{}?market=AU", id))
                    .await?
                    .json::<Value>()
                    .await?;
                match json.get("preview_url") {
                    Some(url_value) => match url_value {
                        Value::String(url) => {
                            trace!("Got preview_url from spotify. {:?}", url);
                            return Ok((url.to_owned(), 0.0));
                        }
                        _ => {
                            warn!("Preview URL is not a string");
                            trace!("{:?}", json);
                            return Err(MiitopiaError::Spotify(SpotifyError::NotFound));
                        }
                    },
                    None => return Err(MiitopiaError::Spotify(SpotifyError::NotFound)),
                }
            }
        }
    }
}
