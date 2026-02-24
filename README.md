# RLIAMP

RLIAMP is a Rust rewrite of [cliamp](https://github.com/bjarneo/cliamp): a retro terminal music player with playlist control, real-time spectrum visualization, and a 10-band EQ.

## Features

- MP3 and M4A playback.
- Real-time 10-band spectrum visualization.
- 10-band parametric EQ.
- Volume and seek controls.
- Playlist navigation with shuffle and repeat modes.
- Unicode-styled terminal UI with ANSI colors.

## Requirements

- Rust toolchain (`cargo`).
- A terminal with ANSI color support.
- `ffmpeg` is recommended for robust M4A/AAC decoding.

## Run

```bash
cargo run -- /path/to/track.mp3
cargo run -- /path/to/*.mp3
cargo run -- "/path/to/track.m4a"
```

## Build

```bash
cargo build --release
./target-user/release/rliamp /path/to/track.mp3
```

Note: this project uses `target-user/` as Cargo target directory (`.cargo/config.toml`).

## Key Bindings

| Key | Action |
|---|---|
| `Space` / `p` | Play / Pause |
| `Enter` | Play selected track |
| `s` | Stop |
| `>` `.` | Next track |
| `<` `,` | Previous track |
| `Left` `Right` | Seek -/+5s |
| `+` `-` | Volume up/down |
| `Tab` | Toggle focus (Playlist / EQ) |
| `j` `k` / `Up` `Down` | Playlist move / EQ band adjust |
| `h` `l` | EQ cursor left/right |
| `r` | Cycle repeat (Off / All / One) |
| `z` | Toggle shuffle |
| `q` | Quit |
