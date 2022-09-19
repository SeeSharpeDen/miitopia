![profile](./resources/discord-profile.png)

# miitopia discord bot

This discord bot will add random snippets of the miitopia sound track to images
and videos sent to it.

## Setup

1. [Install rust](https://www.rust-lang.org/tools/install) and
   [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html).

2. Clone this repo. `git clone git@github.com:SeeSharpeDen/miitopia.git` and cd
   to `miitopia`

3. obtain the miitopia sound track and make sure it's the correct format. See
   [soundtrack](#soundtrack)

4. Set the `DISCORD_TOKEN` environment variable with your token and run
   `cargo run`.
   > Alternatively `DISCORD_TOKEN="[ Token Goes Here ] cargo run`

## Soundtrack

The miitopia soundtrack you've downloaded very likely isn't in the format
miitopia is expecting.

The audio files must be an `ogg` file with the `.ogg` extension. They also need
a **single** track (no album art) with the `vorbis` or `opus` codec.

**Example:**

```
$ ffprobe -hide_banner miitopia_001-A-Lively-Inn.flac.ogg
Input #0, ogg, from 'miitopia_001-A-Lively-Inn.flac.ogg':
  Duration: 00:01:54.95, start: 0.000000, bitrate: 100 kb/s
  Stream #0:0: Audio: vorbis, 32728 Hz, stereo, fltp, 112 kb/s
    Metadata:
      encoder         : Lavc58.134.100 libvorbis
      ALBUM           : Miitopia Original Soundtrack
      ARTIST          : Toshiyuki Sudo, Shinji Ushiroda, Yumi Takahashi, Megumi Inoue
      COMPOSER        : Toshiyuki Sudo, Shinji Ushiroda, Yumi Takahashi, Megumi Inoue
      DATE            : 2021
      GENRE           : Game Soundtrack
      title           : A Lively Inn
      track           : 1
```

The audio files need to be stored in the `resources/music` directory.

**Example:**

```
resources/
└── music
    ├── miitopia_001-A-Lively-Inn.flac.ogg
    ├── miitopia_002-Mmmmm.flac.ogg
    :   ...
    └── miitopia_336-Nintendo-3DS-Home-Menu-Banner.flac.ogg
```

### Converting

You can convert your existing miitopia/music library to this format with ffmpeg.

The example bash script below converts files ending with `.flac` in the
`~/Music/miitopia/` directory to an `.ogg` file with no video (`-vn`), with the
audio codec of libvorbis (`-acodec libvorbis`).

```bash
#!/bin/bash
for file in ~/Music/miitopia/*.flac
do
    ffmpeg -i $file -vn -acodec libvorbis resources/music/$(basename $file).ogg
done
```

This script can be condensed to a single line like below.

```bash
for file in ~/Music/miitopia/*.flac; do ffmpeg -i $file -vn -acodec libvorbis $(basename $file).ogg; done
```
