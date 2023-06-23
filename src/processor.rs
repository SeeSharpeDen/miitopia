use std::{
    borrow::Cow,
    io::Write,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use ffmpeg_cli::{FfmpegBuilder, File, Parameter};
use glob::glob;
use human_repr::{HumanCount, HumanDuration};
use indexmap::IndexMap;
use log::debug;
use ogg_metadata::{read_format, AudioMetadata};
use rand::{rngs::SmallRng, SeedableRng};
use serenity::{
    futures,
    http::CacheHttp,
    model::prelude::{Attachment, AttachmentType, Message, MessageReference},
    prelude::*,
};

use crate::{audio_source::AudioSource, error::MiitopiaError, MAX_AUDIO_LENGTH};

// TODO: Make this async.
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

                        // Ignore tracks that are too short.
                        if secs < MAX_AUDIO_LENGTH {
                            let path = path_clone.display();
                            log::info!(
                                "Ignoring '{path}'. Duration: {secs}s, Minimum: {MAX_AUDIO_LENGTH}."
                            );
                        } else if !map.contains_key(&path_clone) {
                            // Insert our path if it doesn't already exist.
                            map.insert(path_clone, secs);
                        }
                    }
                }
            }
        }
    }
    map
}
pub struct JobResult {
    pub audio_file: String,
    pub attachment: Attachment,
    pub stderr: Option<String>,
    pub output_file: Vec<u8>,
    pub job_time: Duration,
}

pub async fn apply_music(
    audio_file: String,
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
            File::new(audio_file.as_str())
                .option(Parameter::KeyValue("ss", start_str.as_str()))
                .option(Parameter::KeyValue("t", duration_str.as_str())),
        );

    // Get the mimetype of the attachment.
    let mimetype = match &attachment.content_type {
        Some(mimetype) => mimetype,
        None => return Err(MiitopiaError::InvalidFileType),
    };

    let mut shortest = true;

    // Depending on what kind of file we get, we need to do different things.
    let mut ff_builder = match mimetype.as_str() {
        "image/png" | "image/jpeg" | "image/webp" | "image/bmp" => {
            shortest = false;
            ff_builder.input(
                File::new("-")
                    .option(Parameter::KeyValue("f", "image2pipe"))
                    .option(Parameter::KeyValue("framerate", "24")),
            )
        }
        "image/gif" => ff_builder.input(
            File::new("-")
                .option(Parameter::KeyValue("f", "gif"))
                .option(Parameter::KeyValue("stream_loop", "-1")),
        ),
        "video/webm" => ff_builder.input(File::new("-").option(Parameter::KeyValue("f", "webm"))),
        mime => return Err(MiitopiaError::UnsupportedFileType(mime.to_string())),
    };

    // Create our output
    let mut output = File::new("-")
        .option(Parameter::KeyValue("f", "webm"))
        .option(Parameter::KeyValue("vf", "format=yuv420p"))
        .option(Parameter::KeyValue("map", "0:a:0"))
        .option(Parameter::KeyValue("map", "1:v:0"))
        .option(Parameter::KeyValue("threads", "4"));

    if shortest {
        output = output.option(Parameter::Single("shortest"));
    }

    ff_builder = ff_builder
        .output(output)
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
    debug!("{}{}", cmd.get_program().to_str().unwrap(), arg_str);

    // Start ffmpeg.
    let mut child = cmd.spawn()?;

    // Take stdin and write downloaded file in another thread.
    let mut stdin = child.stdin.take().expect("Failed to get stdin");
    std::thread::spawn(move || {
        stdin
            .write_all(&source_bytes)
            .expect("Failed to write to stdin");
    });

    // Read our ffmpeg output.
    let output = child.wait_with_output()?;
    if !output.status.success() {
        // Create a string from the error of ffmpeg then trim the whitespace.
        let err_str = String::from_utf8_lossy(&output.stderr);
        let err_str = err_str.trim();

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

pub async fn process_message(ctx: &Context, msg: &Message) -> Result<(), Vec<MiitopiaError>> {
    let _ = ctx.http().broadcast_typing(msg.channel_id.0);

    let typing = match msg.channel_id.start_typing(&ctx.http) {
        Ok(typing) => Some(typing),
        Err(reason) => {
            log::warn!("Failed to start 'typing'. Reason: {reason}");
            None
        }
    };

    debug!("{}: {}", msg.author.name, msg.content_safe(&ctx.cache));

    // Setup our rng.
    let mut rng = SmallRng::from_entropy();

    // Get the content of the discord message.
    let msg_content = &msg.content_safe(&ctx.cache);

    // Find out where our audio is coming from. Url, Spotify or Miitopia?
    let source = AudioSource::from_msg_content(&msg_content);
    log::trace!("Using {} AudioSource", source);

    let mut errors: Vec<MiitopiaError> = vec![];

    // Start processing the attachments.
    let mut raw_futures = Vec::new();
    for attachment in msg.attachments.clone() {
        let track = source.get_track(&ctx.data, &mut rng).await;
        match track {
            Ok((path, start)) => {
                raw_futures.push(apply_music(path, start, MAX_AUDIO_LENGTH, attachment))
            }
            Err(err) => {
                log::error!("Failed to get track: {:?} for {}", err, msg.id);
                errors.push(err);
            }
        }
    }

    let unpin_futures: Vec<_> = raw_futures.into_iter().map(Box::pin).collect();
    let mut futures = unpin_futures;

    while !futures.is_empty() {
        match futures::future::select_all(futures).await {
            (Ok(job), _index, remaining) => {
                futures = remaining;

                // TODO: Don't print this (clone stderr!!) if env_logger isn't logging info.
                log::info!(
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
                    log::warn!("Error sending message: {:?}", why);
                }
            }
            (Err(error), _index, remaining) => {
                // Update the futures.
                futures = remaining;

                // Print the error to the console.
                log::error!("Error: {}", error);

                errors.push(error);
            }
        }
    }

    if let Some(typing) = typing {
        let _ = typing.stop();
    }

    // Return the errors if there's errors.
    if errors.len() > 0 {
        Err(errors)
    } else {
        Ok(())
    }
}
