use core::fmt;
use std::io;

use serenity::prelude::*;

#[derive(Debug)]
pub enum MiitopiaError {
    Serenity(SerenityError),
    Ffmpeg(String),
    Io(io::Error),
    InvalidFileType,
    UnsupportedFileType,
}

impl fmt::Display for MiitopiaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MiitopiaError::Serenity(e) => write!(f, "Serenity Error: {}", e),
            MiitopiaError::Ffmpeg(e) => write!(f, "Ffmpeg Error: {}", e),
            MiitopiaError::Io(e) => write!(f, "IO Error: {}", e),
            MiitopiaError::InvalidFileType => write!(f, "Invalid File Type"),
            MiitopiaError::UnsupportedFileType => write!(f, "File Type not supported."),
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
