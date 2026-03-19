# RLIAMP
<img width="1710" height="1107" alt="Screenshot 2026-02-26 at 9 44 19 PM" src="https://github.com/user-attachments/assets/6edb229a-d1b1-4f6b-af50-7617e1608976" />

RLIAMP is a Rust rewrite of [cliamp](https://github.com/bjarneo/cliamp): a retro terminal music player with a real-time visualizer, 10-band EQ, and keyboard-first controls.

## Upstream Sync Status

This branch is synced with key upstream updates through **2026-03-19** (including recent upstream `v1.21.x` UX/stability work that is practical in the Rust branch) for:

- recursive folder scanning
- wider/centered UI refresh
- search mode
- queue (play next)
- EQ presets
- mono output toggle
- URL / M3U / PLS / podcast RSS input handling
- local playlist file expansion for `.m3u`, `.m3u8`, `.pls`
- gapless playback with automatic next-track preload
- yt-dlp input support (SoundCloud / YouTube / Bandcamp / Bilibili / NetEase URLs supported by local `yt-dlp`)
- Xiaoyuzhou episode page resolution
- visualizer mode toggle (`Neon` / `Bricks` / `Columns` / `Wave` / `Scatter` / `Flame` / `Retro` / `Matrix` / `Binary` / `Snow` / `Off`)
- full-screen visualizer mode (`V`)
- interactive keymap (`Ctrl+K`, supports up/down navigation)
- theme picker (`t`)
- expanded built-in themes: `Neo Mint`, `tokyo-night`, `nord`, `gruvbox`, `rose-pine`, `catppuccin`, `kanagawa`, `everforest`, `hackerman`, `vantablack`, etc.
- track info overlay (`i`)
- lyrics overlay (`y`) with synced timestamp follow + manual scroll fallback
- runtime URL load overlay (`U`) for direct stream/M3U/PLS/feed links
- runtime YouTube / SoundCloud find (`f` / `F`) with queue-next behavior
- jump to time (`J`)
- file browser overlay (`o`) with local folder drill-down, multi-select, and replace mode
- queue manager (`A`) and playlist manager (`p`)
- save local track to `~/Music` (`S`)
- playlist expand/collapse (`x`)
- auto-sized playlist window (default cap `12`, expanded cap `24`)
- compact mode (`--compact` / `compact = true`)
- resume playback for local tracks across restarts
- radio provider with custom `~/.config/rliamp/radios.toml` support
- multi-provider browser with runtime provider switching (`Radio` / `Navidrome` / `YouTube` / `YouTube Music`)
- auth-capable provider flow for YouTube / YouTube Music with cached credentials
- provider playlist load now replaces current queue before autoplay
- async yt-dlp URL resolution with timeout protection and loading elapsed status
- live/realtime stream unpause now reconnects instead of resuming stale buffered audio
- CLI flags: `--help`, `--version`, `--volume`, `--shuffle`, `--repeat`, `--mono/--no-mono`, `--theme`, `--visualizer`, `--compact`, `--provider`, `--eq-preset`, `--sample-rate`, `--buffer-ms`, `--resample-quality`, `--bit-depth`, `--auto-play`
- audio output tuning: preferred sample rate, buffer size, resample quality, and FFmpeg PCM bit depth
- ffmpeg fallback decode when Symphonia fails (including unsupported WAV variants)
- Navidrome provider integration (`[navidrome]` config section + env fallback)

Pending upstream sync priorities (as of **2026-03-03**, upstream `v1.12.3`~`v1.13.1`):
- see `docs/upstream-sync-priority-2026-03-03.md`

## Features

- Local playback: `mp3`, `wav`, `flac`, `ogg`, `m4a`, `aac`, `m4b`, `m4p`, `alac`, `wma`, `opus`.
- URL playback for direct HTTP/HTTPS audio links.
- Local and remote M3U/PLS playlist expansion, plus podcast RSS feed support.
- SoundCloud / YouTube / Bandcamp / Bilibili URL support via `yt-dlp`.
- Xiaoyuzhou episode page resolution to playable podcast audio.
- Gapless playback for local file queues (auto-preload next track).
- Real-time 10-band spectrum visualization with eleven modes (`Neon`, `Bricks`, `Columns`, `Wave`, `Scatter`, `Flame`, `Retro`, `Matrix`, `Binary`, `Snow`, `Off`).
- Full-screen visualizer mode (`V`), plus interactive keymap/theme/info overlays.
- Lyrics overlay (`y`) with auto-follow for timestamped lyrics.
- File browser overlay (`o`) with recursive local file/folder import.
- Runtime URL input (`U`) to load stream/playlist/feed links without restart.
- Runtime YouTube / SoundCloud find (`f` / `F`) with queue-next behavior.
- Jump to time (`J`) for local tracks.
- Shortcut hints (content inside `[...]`) are theme-accent highlighted.
- Queue manager (`A`) and playlist manager (`p`) overlays.
- Save current local track to `~/Music` (`S`).
- 10-band parametric EQ with built-in presets.
- Configurable large seek jump via `Shift+Left` / `Shift+Right` (`seek_large_step_sec`).
- Compact mode (`--compact`) caps the main player frame at 80 columns.
- Playlist height auto-fits the terminal (default cap `12`, expanded cap `24` with `x`).
- Local-track resume playback is restored automatically on next launch.
- CLI flags: `--help`, `--version`, `--volume`, `--shuffle`, `--repeat`, `--mono/--no-mono`, `--theme`, `--visualizer`, `--compact`, `--provider`, `--eq-preset`, `--sample-rate`, `--buffer-ms`, `--resample-quality`, `--bit-depth`, `--auto-play`.
- Bilingual UI (`English` / `中文`) with runtime toggle.
- Custom EQ quick modes (`1`-`6`) including `Engineer`.
- Queue, search, shuffle, repeat, mono, seek, and volume controls.
- Multi-provider browser with `Radio` always available plus optional `Navidrome`, `YouTube`, and `YouTube Music`.
- YouTube / YouTube Music provider browser with OAuth sign-in and playlist classification cache.
- Optional Navidrome playlist loading via config section or environment variables.
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
cargo run -- --theme tokyo-night /path/to/Music
cargo run -- --visualizer Matrix --sample-rate 48000 --buffer-ms 120 /path/to/Music
cargo run -- --compact --auto-play /path/to/Music

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

# yt-dlp / Xiaoyuzhou sources
cargo run -- "https://www.bilibili.com/video/BV..."
cargo run -- "https://www.xiaoyuzhoufm.com/episode/..."

# provider / search
cargo run -- --provider radio
cargo run -- --provider navidrome
cargo run -- --provider youtube
cargo run -- --provider ytmusic
cargo run -- search "never gonna give you up"
cargo run -- search-sc "lofi hip hop"

# SoundCloud / YouTube / Bandcamp (requires yt-dlp)
cargo run -- "https://soundcloud.com/user/sets/playlist"
cargo run -- "https://www.youtube.com/watch?v=VIDEO_ID"
cargo run -- "https://artist.bandcamp.com/album/album-name"

# provider mode from config file (recommended)
# add this section to ~/.config/rliamp/config.toml:
# [navidrome]
# url = "https://navidrome.example.com"
# user = "alice"
# password = "secret"
cargo run --

# provider mode via env fallback
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

Release publish checklist (Homebrew + ZeroBrew):
- `docs/release-homebrew-zerobrew.md`

## Configuration

```bash
mkdir -p ~/.config/rliamp
cp config.toml.example ~/.config/rliamp/config.toml
```

## Navidrome

Configure provider mode with either `~/.config/rliamp/config.toml` or env vars.

Audio quality knobs can be set in `~/.config/rliamp/config.toml`:

```toml
sample_rate = 48000
buffer_ms = 120
resample_quality = 4
bit_depth = 32
compact = true
```

Notes:
- `sample_rate` asks `cpal` for the closest supported output rate on the active device.
- `buffer_ms` requests a fixed speaker buffer when the device reports a supported range.
- `resample_quality` maps to `1=Nearest`, `2=Linear`, `3=Cubic`, `4=Lanczos`.
- `bit_depth` affects FFmpeg-decoded inputs and streams (`16` or `32`).

## YouTube / YouTube Music

Configure provider mode with either `~/.config/rliamp/config.toml` or env vars:

```toml
[ytmusic]
client_id = "google-oauth-desktop-client-id"
client_secret = "google-oauth-desktop-client-secret"
# cookies_from = "chrome"   # optional, for private/uploaded tracks via yt-dlp
```

Environment fallback:

```bash
export YTMUSIC_CLIENT_ID="google-oauth-desktop-client-id"
export YTMUSIC_CLIENT_SECRET="google-oauth-desktop-client-secret"
# export YTMUSIC_COOKIES_FROM="chrome"
```

Notes:
- Enable `YouTube Data API v3` in your Google Cloud project.
- OAuth callback must allow `http://127.0.0.1:19873/callback`.
- First open of the provider will ask you to sign in in the browser.
- Library playlists are split into `YouTube Music` and `YouTube` by cached category classification.

## Spotify (Experimental)

Spotify support is intentionally not part of the default build.

Reasons:
- it requires rebuilding with `--features spotify-experimental`
- Spotify's current platform policy blocks this path without Spotify Premium
- Spotify local callback URIs must use `127.0.0.1`, not `localhost`

If you explicitly want to experiment with it anyway:

```bash
SPOTIFY_CLIENT_ID="spotify-app-client-id" \
SPOTIFY_REDIRECT_URI="http://127.0.0.1:8888/callback" \
cargo run --features spotify-experimental -- --provider spotify
```

Config file (takes precedence):

```bash
[navidrome]
url = "https://navidrome.example.com"
user = "alice"
password = "secret"
# token = "optional-token"  # optional alternative to password
```

Environment fallback:

```bash
export NAVIDROME_URL="https://navidrome.example.com"
export NAVIDROME_USER="alice"
export NAVIDROME_PASS="secret"
# or: export NAVIDROME_TOKEN="token-value"
```

Then run:

```bash
./target-user/release/rliamp
```

## Radio Provider

`Radio` is available by default. Add custom stations by copying:

```bash
cp radios.toml.example ~/.config/rliamp/radios.toml
```

Inside provider mode:
- `Left` / `Right`: switch provider
- `Up` / `Down`: move playlist selection
- `Enter`: load selected remote playlist
- `r`: reload playlists (retry after empty/error)
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
| `Shift+Left` `Shift+Right` | Large seek step (configurable, local tracks) |
| `J` | Jump to time |
| `o` | Open file browser |
| `+` `-` | Volume up/down |
| `m` | Toggle mono |
| `g` | Toggle matrix background |
| `e` | Cycle EQ preset |
| `t` | Choose theme |
| `c` | Cycle visualizer mode (Neon / Bricks / Columns / Wave / Scatter / Flame / Retro / Matrix / Binary / Snow / Off) |
| `V` | Toggle full-screen visualizer |
| `1` `2` `3` `4` `5` `6` | Apply custom EQ mode |
| `u` | Toggle UI language (EN / ZH) |
| `i` | Track info / metadata overlay |
| `y` | Lyrics overlay (synced/manual scroll) |
| `U` | Load URL at runtime |
| `f` `F` | Find on YouTube / SoundCloud |
| `a` | Toggle queue for selected track |
| `A` | Queue manager |
| `p` | Playlist manager |
| `x` | Expand/collapse playlist |
| `/` | Search playlist |
| `Tab` | Toggle focus (Playlist / EQ) |
| `N` | Open provider browser |
| `Left` `Right` in provider view | Switch provider |
| `Enter` in auth-needed provider view | Sign in and reload playlists |
| `Esc` / `b` | Back to provider view (when provider is configured) |
| `j` `k` / `Up` `Down` | Playlist move / EQ band adjust |
| `h` `l` | EQ cursor left/right |
| `Enter` | Play selected track |
| `r` | Cycle repeat (Off / All / One) |
| `z` | Toggle shuffle |
| `Ctrl+K` | Show keymap |
| `q` | Quit |
