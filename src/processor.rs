use std::{io::Write, path::PathBuf, process::Stdio};

use ffmpeg_cli::{FfmpegBuilder, File, Parameter};
use glob::glob;
use indexmap::IndexMap;
use ogg_metadata::{read_format, AudioMetadata};
use rand::{prelude::SmallRng, Rng};
use serenity::model::channel::Attachment;

use crate::error::MiitopiaError;

pub fn scan_music() -> IndexMap<PathBuf, f32> {
    let mut map = IndexMap::new();

    // Iterate over all .ogg files in the music directory.
    for entry in glob("./resources/music/*.ogg").expect("Failed to read glob pattern for music") {
        if let Ok(path) = entry {
            // Clone the path because opening the file consumes our path.
            let path_clone = path.clone();

            // Open the file and read the formats and get the first item.
            if let Ok(mut file) = std::fs::File::open(path) {
                if let Ok(formats) = read_format(&mut file) {
                    let format = &formats[0];

                    // Get the duration if that format supports a duration.
                    if let Some(duration) = match format {
                        ogg_metadata::OggFormat::Vorbis(track) => track.get_duration(),
                        ogg_metadata::OggFormat::Opus(track) => track.get_duration(),
                        _ => None,
                    } {
                        let secs = duration.as_secs_f32();

                        // Insert our path if it doesn't already exist.
                        if !map.contains_key(&path_clone) {
                            map.insert(path_clone, secs);
                        }
                    }
                }
            }
        }
    }
    map
}
pub fn get_rand_track(
    tracks: &IndexMap<PathBuf, f32>,
    random: &mut SmallRng,
) -> Option<(PathBuf, f32)> {
    let index = random.gen_range(0..tracks.len());
    if let Some((path, duration)) = tracks.get_index(index) {
        return Some((path.clone(), duration.clone()));
    }
    return None;
}

pub async fn apply_music(
    audio_file: PathBuf,
    start: f32,
    duration: f32,
    attachment: Attachment,
) -> Result<Vec<u8>, MiitopiaError> {
    let duration_str = duration.to_string();
    let start_str = start.to_string();
    // Create our ffmpeg builder.
    let ff_builder = FfmpegBuilder::new()
        .option(Parameter::Single("hide_banner"))
        .option(Parameter::KeyValue("loglevel", "error"))
        .option(Parameter::Single("nostdin"))
        .input(
            File::new(
                audio_file
                    .to_str()
                    .expect("Failed to convert PathBuf to str"),
            )
            .option(Parameter::KeyValue("ss", start_str.as_str()))
            .option(Parameter::KeyValue("t", duration_str.as_str())),
        );

    // Get the mimetype of the attachment.
    let mimetype = match &attachment.content_type {
        Some(mimetype) => mimetype,
        None => return Err(MiitopiaError::InvalidFileType),
    };

    // Depending on what kind of file we get, we need to do different things.
    let ff_builder = match mimetype.as_str() {
        "image/png" | "image/jpeg" | "image/webp" | "image/bmp" => ff_builder.input(
            File::new("-")
                // For images, some players don't like framerates < 1 so set one.
                .option(Parameter::KeyValue("f", "image2pipe"))
                .option(Parameter::KeyValue("framerate", "24")),
        ),
        "image/gif" => ff_builder.input(
            File::new("-")
                // Loop the gif until the music ends.
                .option(Parameter::KeyValue("f", "gif_pipe"))
                .option(Parameter::KeyValue("stream_loop", "-1"))
                .option(Parameter::KeyValue("loop", "0")),
        ),
        _ => return Err(MiitopiaError::UnsupportedFileType),
    };

    let ff_builder = ff_builder
        .output(
            File::new("-")
                .option(Parameter::KeyValue("f", "webm"))
                .option(Parameter::KeyValue("map", "0:a:0"))
                .option(Parameter::KeyValue("map", "1:v:0")),
        )
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .stderr(Stdio::piped());

    // Download our source file.
    let source_bytes = attachment.download().await?;

    // Start ffmpeg.
    let mut child = ff_builder.to_command().spawn()?;

    // Write our downloaded file to stdin.
    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    stdin.write_all(&source_bytes)?;
    drop(stdin);

    // Read our ffmpeg output.
    let output = child.wait_with_output()?;
    if !output.status.success() {
        // Create a string from the error of ffmpeg then trim the whitespace.
        let err_str = String::from_utf8_lossy(&output.stderr);
        let err_str = err_str.trim();

        println!("ffmpeg error: {}", err_str);
        return Err(MiitopiaError::Ffmpeg(err_str.to_string()));
    }

    return Ok(output.stdout);
}
fn split_mime(mimetype: String) -> Result<(String, String), MiitopiaError> {
    let parts: Vec<&str> = mimetype.split("/").collect();
    if parts.len() != 2 {
        return Err(MiitopiaError::InvalidFileType);
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}
pub async fn apply_music_to_image(
    audio_file: PathBuf,
    start: f32,
    duration: f32,
    attachment: Attachment,
) -> Result<Vec<u8>, MiitopiaError> {
    let duration_str = duration.to_string();
    let start_str = start.to_string();

    // Create our ffmpeg command.
    let ff_builder = FfmpegBuilder::new()
        .option(Parameter::Single("hide_banner"))
        .option(Parameter::KeyValue("loglevel", "error"))
        .option(Parameter::Single("nostdin"))
        // .input(File::new("-").option(Parameter::KeyValue("f", parts[1])))
        .input(
            File::new("-")
                .option(Parameter::KeyValue("f", "image2pipe"))
                .option(Parameter::KeyValue("framerate", "30")),
        )
        .input(
            File::new(
                audio_file
                    .to_str()
                    .expect("Failed to convert PathBuf to str"),
            )
            .option(Parameter::KeyValue("ss", start_str.as_str()))
            .option(Parameter::KeyValue("t", duration_str.as_str())),
        )
        .output(
            File::new("-")
                .option(Parameter::KeyValue("f", "webm"))
                .option(Parameter::KeyValue("map", "0:v:0"))
                .option(Parameter::KeyValue("map", "1:a:0")),
        )
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .stderr(Stdio::piped());

    // Download our source file.
    let source_bytes = attachment.download().await?;

    let mut child = ff_builder.to_command().spawn()?;

    // Write our downloaded file to stdin.
    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    stdin.write_all(&source_bytes)?;
    drop(stdin);

    // Read our ffmpeg output.
    let output = child.wait_with_output()?;
    if !output.status.success() {
        // Create a string from the error of ffmpeg then trim the whitespace.
        let err_str = String::from_utf8_lossy(&output.stderr);
        let err_str = err_str.trim();

        println!("ffmpeg error: {}", err_str);
        return Err(MiitopiaError::Ffmpeg(err_str.to_string()));
    }

    return Ok(output.stdout);
}
