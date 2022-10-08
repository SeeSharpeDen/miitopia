use std::{fmt, path::PathBuf};

use indexmap::IndexMap;
use rand::{prelude::SmallRng, Rng};
use reqwest::header::CONTENT_TYPE;

use crate::{error::MiitopiaError, MAX_LENGTH};

pub enum AudioSource {
    Miitopia,
    Url(String),
}

impl fmt::Display for AudioSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioSource::Miitopia => write!(f, "Miitopia"),
            AudioSource::Url(_) => write!(f, "MyInstants"),
        }
    }
}

impl AudioSource {
    pub async fn validate(&self, tracks: &IndexMap<PathBuf, f32>) -> Option<MiitopiaError> {
        match self {
            AudioSource::Miitopia => {
                if tracks.len() > 0 {
                    None
                } else {
                    Some(MiitopiaError::NoTracks)
                }
            }
            AudioSource::Url(url) => {
                let result = reqwest::get(url).await;
                match result {
                    Ok(result) => {
                        if let Ok(mime) = result.headers()[CONTENT_TYPE].to_str() {
                            match mime {
                                // Make sure we it's a valid sound file.
                                "audio/mpeg" | "audio/ogg" | "audio/vorbis" => return None,
                                content_type => {
                                    return Some(MiitopiaError::UnsupportedFileType(
                                        content_type.to_string(),
                                    ))
                                }
                            }
                        }
                        return Some(MiitopiaError::UnsupportedFileType("Unknown".to_string()));
                    }
                    Err(error) => Some(MiitopiaError::Reqwest(error)),
                }
            }
        }
    }
    pub fn get_track(
        &self,
        tracks: &IndexMap<PathBuf, f32>,
        rng: &mut SmallRng,
    ) -> Result<(String, f32), MiitopiaError> {
        match self {
            AudioSource::Miitopia => {
                // Get a random track.
                let index = rng.gen_range(0..tracks.len());
                if let Some((path, track_duration)) = tracks.get_index(index) {
                    // If the track is longer than MAX_LENGTH, return track with a random start time.
                    let start_max = track_duration - track_duration.min(MAX_LENGTH);
                    let mut start = 0.0;
                    if start_max > 0.0 {
                        start = rng.gen_range(0.0..start_max);
                    }
                    return Ok((
                        path.to_owned().into_os_string().into_string().unwrap(),
                        start,
                    ));
                }
                return Err(MiitopiaError::NoTracks);
            }
            AudioSource::Url(url) => Ok((url.to_string(), 0.0)),
        }
    }
}
