use std::{collections::HashMap, fs::File, path::PathBuf};

use glob::glob;
use ogg_metadata::{read_format, AudioMetadata};

pub fn scan_music() -> HashMap<PathBuf, f32> {
    let mut map = HashMap::new();

    // Iterate over all .ogg files in the music directory.
    for entry in glob("./resources/music/*.ogg").expect("Failed to read glob pattern for music") {
        if let Ok(path) = entry {
            // Clone the path because opening the file consumes our path.
            let path_clone = path.clone();

            // Open the file and read the formats and get the first item.
            if let Ok(mut file) = File::open(path) {
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