# RLIAMP

RLIAMP is a Rust rewrite of [cliamp](https://github.com/bjarneo/cliamp): a retro terminal music player with a real-time visualizer, 10-band EQ, and keyboard-first controls.

## Upstream Sync Status

This branch is synced with key upstream updates through **2026-02-25** (feature window from `951e5b1` to `a326ef6`) for:

- recursive folder scanning
- wider/centered UI refresh
- search mode
- queue (play next)
- EQ presets
- mono output toggle
- URL / M3U / podcast RSS input handling
- Navidrome provider integration (`NAVIDROME_URL` / `NAVIDROME_USER` / `NAVIDROME_PASS`)

## Features

- Local playback: `mp3`, `wav`, `flac`, `ogg`, `m4a`, `aac`, `m4b`, `m4p`, `alac`, `wma`, `opus`.
- URL playback for direct HTTP/HTTPS audio links.
- M3U and podcast RSS feed expansion.
- Real-time 10-band spectrum visualization (`▁▂▃▄▅▆▇█`).
- 10-band parametric EQ with built-in presets.
- Bilingual UI (`English` / `中文`) with runtime toggle.
- Custom EQ quick modes (`1`-`6`) including `Engineer`.
- Queue, search, shuffle, repeat, mono, seek, and volume controls.
- Optional Navidrome playlist loading via environment variables.
- Unicode-style ANSI-colored terminal UI.

## Requirements

- Rust toolchain (`cargo`).
- A terminal with ANSI color support.
- `ffmpeg` in `PATH` (required for URL decoding and non-core formats such as AAC/ALAC/WMA/Opus).

## Run

```bash
# local files
cargo run -- /path/to/track.mp3
cargo run -- /path/to/*.mp3

# recursive directory scan
cargo run -- /path/to/Music

# direct URL / M3U / podcast RSS
cargo run -- "https://example.com/song.mp3"
cargo run -- "https://example.com/radio.m3u"
cargo run -- "https://example.com/podcast/feed.xml"

# provider mode (no file arguments required)
NAVIDROME_URL="https://navidrome.example.com" \
NAVIDROME_USER="alice" \
NAVIDROME_PASS="secret" \
cargo run --
```

## Build

```bash
cargo build --release
./target-user/release/rliamp /path/to/track.mp3
```

Note: this project uses `target-user/` as Cargo target directory (`.cargo/config.toml`).

## Configuration

```bash
mkdir -p ~/.config/rliamp
cp config.toml.example ~/.config/rliamp/config.toml
```

## Navidrome

Set all three variables to enable provider mode:

```bash
export NAVIDROME_URL="https://navidrome.example.com"
export NAVIDROME_USER="alice"
export NAVIDROME_PASS="secret"
```

Then run:

```bash
./target-user/release/rliamp
```

Inside provider mode:
- `Up` / `Down`: move playlist selection
- `Enter`: load selected remote playlist
- `Tab`: switch focus back to local playlist/EQ view (after tracks are loaded)

## Custom EQ Modes

Press `1`-`6` at runtime to apply the custom profiles:

- `1` Architect (deep focus)
- `2` Spatial HiFi
- `3` Gym / Drive
- `4` Live Reality
- `5` Theta Sleep
- `6` Engineer

`e` still cycles all presets (built-in + custom).

## Key Bindings

| Key | Action |
|---|---|
| `Space` / `p` | Play / Pause |
| `s` | Stop |
| `>` `.` | Next track |
| `<` `,` | Previous track |
| `Left` `Right` | Seek -/+5s (local tracks) |
| `+` `-` | Volume up/down |
| `m` | Toggle mono |
| `e` | Cycle EQ preset |
| `1` `2` `3` `4` `5` `6` | Apply custom EQ mode |
| `i` | Toggle UI language (EN / ZH) |
| `a` | Toggle queue for selected track |
| `/` | Search playlist |
| `Tab` | Toggle focus (Playlist / EQ) |
| `Esc` / `b` | Back to provider view (when provider is configured) |
| `j` `k` / `Up` `Down` | Playlist move / EQ band adjust |
| `h` `l` | EQ cursor left/right |
| `Enter` | Play selected track |
| `r` | Cycle repeat (Off / All / One) |
| `z` | Toggle shuffle |
| `Ctrl+K` | Show keymap |
| `q` | Quit |
