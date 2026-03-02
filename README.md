# RLIAMP
<img width="1710" height="1107" alt="Screenshot 2026-02-26 at 9 44 19 PM" src="https://github.com/user-attachments/assets/6edb229a-d1b1-4f6b-af50-7617e1608976" />

RLIAMP is a Rust rewrite of [cliamp](https://github.com/bjarneo/cliamp): a retro terminal music player with a real-time visualizer, 10-band EQ, and keyboard-first controls.

## Upstream Sync Status

This branch is synced with key upstream updates through **2026-03-01** (feature window from `1d3e9d6` to `e24a269`) for:

- recursive folder scanning
- wider/centered UI refresh
- search mode
- queue (play next)
- EQ presets
- mono output toggle
- URL / M3U / PLS / podcast RSS input handling
- local playlist file expansion for `.m3u`, `.m3u8`, `.pls`
- gapless playback with automatic next-track preload
- yt-dlp input support (SoundCloud / YouTube / Bandcamp)
- visualizer mode toggle (`Neon` / `Bricks` / `Columns` / `Wave` / `Scatter` / `Flame`)
- full-screen visualizer mode (`V`)
- interactive keymap (`Ctrl+K`, supports up/down navigation)
- theme picker (`t`)
- track info overlay (`i`)
- queue manager (`A`) and playlist manager (`p`)
- save local track to `~/Music` (`S`)
- playlist expand/collapse (`x`)
- provider playlist load now replaces current queue before autoplay
- CLI flags: `--help`, `--version`, `--volume`, `--shuffle`, `--repeat`, `--mono/--no-mono`, `--theme`, `--eq-preset`, `--auto-play`
- ffmpeg fallback decode when Symphonia fails (including unsupported WAV variants)
- Navidrome provider integration (`NAVIDROME_URL` / `NAVIDROME_USER` / `NAVIDROME_PASS`)

## Features

- Local playback: `mp3`, `wav`, `flac`, `ogg`, `m4a`, `aac`, `m4b`, `m4p`, `alac`, `wma`, `opus`.
- URL playback for direct HTTP/HTTPS audio links.
- Local and remote M3U/PLS playlist expansion, plus podcast RSS feed support.
- SoundCloud / YouTube / Bandcamp URL support via `yt-dlp`.
- Gapless playback for local file queues (auto-preload next track).
- Real-time 10-band spectrum visualization with six modes (`Neon`, `Bricks`, `Columns`, `Wave`, `Scatter`, `Flame`).
- Full-screen visualizer mode (`V`), plus interactive keymap/theme/info overlays.
- Queue manager (`A`) and playlist manager (`p`) overlays.
- Save current local track to `~/Music` (`S`).
- 10-band parametric EQ with built-in presets.
- CLI flags: `--help`, `--version`, `--volume`, `--shuffle`, `--repeat`, `--mono/--no-mono`, `--theme`, `--eq-preset`, `--auto-play`.
- Bilingual UI (`English` / `中文`) with runtime toggle.
- Custom EQ quick modes (`1`-`6`) including `Engineer`.
- Queue, search, shuffle, repeat, mono, seek, and volume controls.
- Optional Navidrome playlist loading via environment variables.
- Unicode-style ANSI-colored terminal UI.

## Requirements

- Rust toolchain (`cargo`).
- A terminal with ANSI color support.
- `ffmpeg` in `PATH` (required for URL decoding and non-core formats such as AAC/ALAC/WMA/Opus, and fallback decode).
- `yt-dlp` in `PATH` (required only for SoundCloud / YouTube / Bandcamp URLs).

## Install (Homebrew / ZeroBrew)

```bash
# Homebrew
brew tap 0smboy/rliamp https://github.com/0smboy/rliamp
brew install 0smboy/rliamp/rliamp

# ZeroBrew (macOS: ensure Ruby 3 in PATH first)
brew install ruby
export PATH="/opt/homebrew/opt/ruby/bin:$PATH"
zb install 0smboy/rliamp/rliamp

# ensure installed binaries are discoverable
export PATH="/opt/zerobrew/bin:$PATH"
```

## Cyber Stage Integration

- Method 1 (recommended, isolated): use `arliamp` to launch a dedicated Ghostty session that does not modify global Ghostty/tmux/zsh configs.
  - Repo: https://github.com/0smboy/arliamp
- Method 2 (legacy, global config): use the previous `~/.config/ghostty-run-own` workflow.
  - Files and instructions are archived at:
    - `docs/method2-ghostty-run-own/readme.md`
    - `docs/method2-ghostty-run-own/rliamp-veo-setup.sh`
    - `docs/method2-ghostty-run-own/veo-toggle.sh`
    - `docs/method2-ghostty-run-own/cyber-static.glsl`
    - `docs/method2-ghostty-run-own/cyber-crazy.glsl`
    - `docs/method2-ghostty-run-own/ghostty-config-snippet.conf`

## Run

```bash
# local files
cargo run -- /path/to/track.mp3
cargo run -- /path/to/*.mp3

# start immediately with overrides
cargo run -- --auto-play --shuffle --volume -5 /path/to/Music
cargo run -- --theme Amber --eq-preset "Rock" /path/to/Music

# recursive directory scan
cargo run -- /path/to/Music

# local M3U / PLS playlist
cargo run -- /path/to/radio.m3u
cargo run -- /path/to/radio.pls

# direct URL / M3U / PLS / podcast RSS
cargo run -- "https://example.com/song.mp3"
cargo run -- "https://example.com/radio.m3u"
cargo run -- "https://example.com/radio.pls"
cargo run -- "https://example.com/podcast/feed.xml"

# SoundCloud / YouTube / Bandcamp (requires yt-dlp)
cargo run -- "https://soundcloud.com/user/sets/playlist"
cargo run -- "https://www.youtube.com/watch?v=VIDEO_ID"
cargo run -- "https://artist.bandcamp.com/album/album-name"

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
| `Space` | Play / Pause |
| `s` | Stop |
| `S` | Save current local track to `~/Music` |
| `>` `.` | Next track |
| `<` `,` | Previous track |
| `Left` `Right` | Seek -/+5s (local tracks) |
| `+` `-` | Volume up/down |
| `m` | Toggle mono |
| `g` | Toggle matrix background |
| `e` | Cycle EQ preset |
| `t` | Choose theme |
| `c` / `v` | Cycle visualizer mode (Neon / Bricks / Columns / Wave / Scatter / Flame) |
| `V` | Toggle full-screen visualizer |
| `1` `2` `3` `4` `5` `6` | Apply custom EQ mode |
| `u` | Toggle UI language (EN / ZH) |
| `i` | Track info / metadata overlay |
| `a` | Toggle queue for selected track |
| `A` | Queue manager |
| `p` | Playlist manager |
| `x` | Expand/collapse playlist |
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
