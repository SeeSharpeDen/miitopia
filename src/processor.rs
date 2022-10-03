use std::{
    io::Write,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use ffmpeg_cli::{FfmpegBuilder, File, Parameter};
use glob::glob;
use indexmap::IndexMap;
use log::{debug, error};
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

pub struct JobResult {
    pub audio_file: PathBuf,
    pub attachment: Attachment,
    pub stderr: Option<String>,
    pub output_file: Vec<u8>,
    pub job_time: Duration,
}

pub async fn apply_music(
    audio_file: PathBuf,
    start: f32,
    duration: f32,
    attachment: Attachment,
) -> Result<JobResult, MiitopiaError> {
    let start_time = Instant::now();
    let duration_str = duration.to_string();
    let start_str = start.to_string();
    // Create our ffmpeg builder.
    let ff_builder = FfmpegBuilder::new()
        .option(Parameter::Single("hide_banner"))
        // .option(Parameter::KeyValue("loglevel", "warning"))
        .option(Parameter::KeyValue("loglevel", "error"))
        .option(Parameter::Single("nostdin"))
        .input(
            File::new(
                audio_file
                    .to_str()
                    .expect("Failed to convert PathBuf to str"),
            )
            .option(Parameter::KeyValue("ss", start_str.as_str())),
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
                .option(Parameter::KeyValue("f", "image2pipe"))
                .option(Parameter::KeyValue("framerate", "24")),
        ),
        "image/gif" => ff_builder.input(
            File::new("-")
                .option(Parameter::KeyValue("f", "gif"))
                .option(Parameter::KeyValue("stream_loop", "-1")),
        ),
        mime => return Err(MiitopiaError::UnsupportedFileType(mime.to_string())),
    };
    let ff_builder = ff_builder
        .output(
            File::new("-")
                .option(Parameter::KeyValue("f", "webm"))
                .option(Parameter::KeyValue("map", "0:a:0"))
                .option(Parameter::KeyValue("map", "1:v:0"))
                .option(Parameter::KeyValue("t", duration_str.as_str()))
                .option(Parameter::KeyValue("threads", "4")),
        )
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .stderr(Stdio::piped());

    // Download our source file.
    let source_bytes = attachment.download().await?;

    // Start ffmpeg.
    let mut cmd = ff_builder.to_command();

    // TODO: Only run this if env_logger is logging debug messages.
    // Get all the args from ffmpeg and join them together into one single string.
    let mut arg_str = String::new();
    for arg in cmd.get_args() {
        arg_str.push(' ');
        arg_str.push_str(arg.to_str().unwrap_or_default());
    }
    debug!("{} {}", cmd.get_program().to_str().unwrap(), arg_str);

    // let mut child = ff_builder.to_command().spawn()?;
    let mut child = cmd.spawn()?;

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

        error!("ffmpeg error: {}", err_str);
        return Err(MiitopiaError::Ffmpeg(err_str.to_string()));
    }

    let mut stderr = String::from_utf8(output.stderr).ok();
    if stderr.is_some() {
        let str = stderr.unwrap();
        let str = str.trim();
        if str.is_empty() {
            stderr = None;
        } else {
            stderr = Some(str.to_string())
        }
    }

    Ok(JobResult {
        job_time: start_time.elapsed(),
        attachment,
        audio_file,
        output_file: output.stdout,
        stderr: stderr,
    })
}
