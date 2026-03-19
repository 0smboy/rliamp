mod background;
mod config;
mod local_source;
mod lyrics;
mod navidrome;
mod player;
mod playlist;
mod provider;
mod radio;
mod resume;
mod runtime_url;
#[cfg(feature = "spotify-experimental")]
mod spotify;
mod ui;
mod visualizer;
mod ytdlp;
mod ytmusic;

use anyhow::{anyhow, Context, Result};
use glob::glob;
use local_source::resolve_local_input;
use navidrome::NavidromeClient;
use playlist::{is_url, Playlist, Track};
use provider::ProviderEntry;
use radio::RadioProvider;
#[cfg(feature = "spotify-experimental")]
use spotify::SpotifyProvider;
use std::path::{Path, PathBuf};
use std::process;
use ytmusic::YtProviderBundle;

#[derive(Debug, Clone, Default)]
struct CliOverrides {
    volume: Option<f32>,
    shuffle: Option<bool>,
    repeat: Option<String>,
    mono: Option<bool>,
    theme: Option<String>,
    visualizer: Option<String>,
    compact: Option<bool>,
    provider: Option<String>,
    eq_preset: Option<String>,
    sample_rate: Option<u32>,
    buffer_ms: Option<u32>,
    resample_quality: Option<u8>,
    bit_depth: Option<u16>,
    auto_play: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliAction {
    Run,
    Help,
    Version,
}

const HELP_TEXT: &str = r#"rliamp — retro terminal music player

Usage: rliamp [flags] <file|folder|url> [...]

Flags:
  --volume <dB>          Volume in dB, range [-30, +6]
  --shuffle              Start with shuffle enabled
  --repeat <off|all|one>
  --mono / --no-mono
  --theme <name>         Theme name (e.g. tokyo-night, nord, gruvbox, rose-pine)
  --visualizer <name>    Startup visualizer mode
  --compact              Cap the main frame width to 80 columns
  --provider <name>      Provider: radio, navidrome, youtube, or ytmusic
  --eq-preset <name>     EQ preset name (e.g. Bass Boost)
  --sample-rate <Hz>     Preferred output sample rate
  --buffer-ms <ms>       Preferred output buffer size
  --resample-quality <1-4>
  --bit-depth <16|32>    FFmpeg PCM precision
  --auto-play            Start playback immediately
  --help, -h             Show this help message
  --version, -v          Show the current version

Examples:
  rliamp --shuffle --volume -5 ~/Music
  rliamp --repeat all --mono track.mp3
  rliamp --theme Amber --eq-preset "Rock" ~/Music
  rliamp track.mp3 song.flac ~/Music
  rliamp ~/radio-stations.m3u
  rliamp ~/radio-stations.pls
  rliamp https://example.com/song.mp3
  rliamp http://radio.example.com/stream.m3u
  rliamp http://radio.example.com/stream.pls
  rliamp https://example.com/podcast/feed.xml
  rliamp https://soundcloud.com/user/sets/playlist
  rliamp https://www.youtube.com/watch?v=...
  rliamp https://www.bilibili.com/video/BV...
  rliamp https://artist.bandcamp.com/album/...
  rliamp https://www.xiaoyuzhoufm.com/episode/...

Environment:
  NAVIDROME_URL, NAVIDROME_USER, NAVIDROME_PASS, NAVIDROME_TOKEN
  YTMUSIC_CLIENT_ID, YTMUSIC_CLIENT_SECRET, YTMUSIC_COOKIES_FROM
  (env fallback when matching config sections are not set in ~/.config/rliamp/config.toml)

Formats:
  mp3, wav, flac, ogg, m4a, aac, opus, wma
  (aac/opus/wma and some streams need ffmpeg)
"#;

fn run() -> Result<()> {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let (action, overrides, args) = parse_cli_args(raw_args)?;
    match action {
        CliAction::Help => {
            println!("{HELP_TEXT}");
            return Ok(());
        }
        CliAction::Version => {
            println!("rliamp {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        CliAction::Run => {}
    }

    let cfg = config::Config::load().unwrap_or_default();
    let provider_name = overrides
        .provider
        .clone()
        .unwrap_or_else(|| cfg.provider.clone());
    let (providers, default_provider, ytdlp_cookies_from) = build_providers(&cfg, &provider_name)?;
    ytdlp::set_cookies_from(ytdlp_cookies_from);

    if args.is_empty() && providers.is_empty() {
        return Err(anyhow!(
            "usage: rliamp <file|folder|url> [...] or configure a provider via ~/.config/rliamp/config.toml\n\nexamples:\n  rliamp song.mp3\n  rliamp ~/Music\n  rliamp ~/radio-stations.m3u\n  rliamp ~/radio-stations.pls\n  rliamp https://example.com/stream.m3u\n  rliamp https://soundcloud.com/user/sets/playlist\n\nprovider config sections:\n  [navidrome]\n  url = \"https://navidrome.example.com\"\n  user = \"alice\"\n  password = \"secret\"\n\n  [ytmusic]\n  client_id = \"google-client-id\"\n  client_secret = \"google-client-secret\"\n\nprovider env fallback:\n  NAVIDROME_URL NAVIDROME_USER NAVIDROME_PASS (or NAVIDROME_TOKEN)\n  YTMUSIC_CLIENT_ID YTMUSIC_CLIENT_SECRET (optional: YTMUSIC_COOKIES_FROM)\n\noptional tools:\n  yt-dlp (for SoundCloud/YouTube/Bandcamp URLs)\n\nexperimental:\n  Spotify is behind --features spotify-experimental and currently also requires Spotify Premium."
        ));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    let mut resolved_tracks: Vec<Track> = Vec::new();

    let args = normalize_search_args(args)?;

    for arg in args {
        if is_url(&arg) {
            resolved_tracks.extend(
                runtime_url::resolve_runtime_url(&arg)
                    .with_context(|| format!("resolving remote input: {arg}"))?,
            );
            continue;
        }

        match glob(&arg) {
            Ok(paths) => {
                let mut matched = false;
                for entry in paths.flatten() {
                    matched = true;
                    resolve_local_input(&entry, &mut files, &mut resolved_tracks)?;
                }
                if !matched {
                    resolve_local_input(Path::new(&arg), &mut files, &mut resolved_tracks)?;
                }
            }
            Err(_) => resolve_local_input(Path::new(&arg), &mut files, &mut resolved_tracks)?,
        }
    }

    files.sort();
    files.dedup();
    let mut playlist = Playlist::new();
    playlist.add(
        files
            .into_iter()
            .map(|path| Track::from_path(path.to_string_lossy().to_string())),
    );
    playlist.add(resolved_tracks);

    let mut resume_target = None;
    if let Some(state) = resume::load().unwrap_or(None) {
        if let Some(idx) = playlist.tracks().iter().position(|track| {
            !track.path.trim().is_empty()
                && track.path == state.path
                && !track.ytdlp
                && !track.is_live()
        }) {
            playlist.set_index(idx);
            resume_target = Some((state.path, state.secs));
        }
    }

    if playlist.len() == 0 && providers.is_empty() {
        return Err(anyhow!("no playable files found"));
    }

    let volume = overrides.volume.unwrap_or(cfg.volume);
    let repeat = overrides.repeat.unwrap_or(cfg.repeat);
    let shuffle = overrides.shuffle.unwrap_or(cfg.shuffle);
    let mono = overrides.mono.unwrap_or(cfg.mono);
    let eq_preset = overrides.eq_preset.unwrap_or(cfg.eq_preset);
    let theme = overrides.theme.or(cfg.theme);
    let visualizer = overrides.visualizer.or(cfg.visualizer.clone());
    let compact = overrides.compact.unwrap_or(cfg.compact);
    let sample_rate = overrides.sample_rate.or(cfg.sample_rate);
    let buffer_ms = overrides.buffer_ms.or(cfg.buffer_ms);
    let resample_quality = overrides.resample_quality.unwrap_or(cfg.resample_quality);
    let bit_depth = overrides.bit_depth.unwrap_or(cfg.bit_depth);

    match repeat.as_str() {
        "all" => playlist.cycle_repeat(),
        "one" => {
            playlist.cycle_repeat();
            playlist.cycle_repeat();
        }
        _ => {}
    }
    if shuffle {
        playlist.toggle_shuffle();
    }

    let player = player::Player::new(player::PlayerOptions {
        sample_rate,
        buffer_ms,
        resample_quality,
        bit_depth,
    })?;
    player.set_volume(volume);
    if mono {
        player.toggle_mono();
    }
    if eq_preset.is_empty() || eq_preset.eq_ignore_ascii_case("custom") {
        for (i, gain) in cfg.eq.into_iter().enumerate() {
            player.set_eq_band(i, gain);
        }
    }

    let mut app = ui::App::new(player, playlist, providers, &default_provider);
    app.set_seek_large_step_sec(cfg.seek_large_step_sec);
    app.set_compact_mode(compact);
    if let Some((path, secs)) = resume_target {
        app.set_resume_state(path, secs);
        app.set_auto_play(true);
    }
    if !eq_preset.is_empty() && !eq_preset.eq_ignore_ascii_case("custom") {
        app.set_eq_preset_by_name(&eq_preset);
    }
    if let Some(theme_name) = theme {
        if !theme_name.trim().is_empty() {
            let _ = app.set_theme_by_name(&theme_name);
        }
    }
    if let Some(mode_name) = visualizer {
        if !mode_name.trim().is_empty() {
            let _ = app.set_visualizer_by_name(&mode_name);
        }
    }
    if overrides.auto_play {
        app.set_auto_play(true);
    }

    let result = app.run();
    match app.resume_state() {
        Some((path, secs)) => {
            let _ = resume::save(&resume::ResumeState { path, secs });
        }
        None => {
            let _ = resume::clear();
        }
    }
    result
}

fn parse_cli_args(args: Vec<String>) -> Result<(CliAction, CliOverrides, Vec<String>)> {
    let mut overrides = CliOverrides::default();
    let mut positional = Vec::new();
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with('-') {
            positional.push(arg.clone());
            i += 1;
            continue;
        }

        match arg.as_str() {
            "--help" | "-h" => return Ok((CliAction::Help, overrides, Vec::new())),
            "--version" | "-v" => return Ok((CliAction::Version, overrides, Vec::new())),
            "--shuffle" => overrides.shuffle = Some(true),
            "--mono" => overrides.mono = Some(true),
            "--no-mono" => overrides.mono = Some(false),
            "--auto-play" => overrides.auto_play = true,
            "--compact" => overrides.compact = Some(true),
            "--volume" => {
                let value = next_arg(&args, &mut i, "--volume")?;
                let parsed = value
                    .parse::<f32>()
                    .map_err(|_| anyhow!("flag --volume: invalid number '{value}'"))?;
                overrides.volume = Some(parsed.clamp(-30.0, 6.0));
            }
            "--repeat" => {
                let value = next_arg(&args, &mut i, "--repeat")?.to_ascii_lowercase();
                if !matches!(value.as_str(), "off" | "all" | "one") {
                    return Err(anyhow!("flag --repeat value must be one of: off, all, one"));
                }
                overrides.repeat = Some(value);
            }
            "--theme" => overrides.theme = Some(next_arg(&args, &mut i, "--theme")?),
            "--visualizer" => overrides.visualizer = Some(next_arg(&args, &mut i, "--visualizer")?),
            "--provider" => {
                overrides.provider =
                    Some(next_arg(&args, &mut i, "--provider")?.to_ascii_lowercase())
            }
            "--eq-preset" => overrides.eq_preset = Some(next_arg(&args, &mut i, "--eq-preset")?),
            "--sample-rate" => {
                let value = next_arg(&args, &mut i, "--sample-rate")?;
                let parsed = value
                    .parse::<u32>()
                    .map_err(|_| anyhow!("flag --sample-rate: invalid integer '{value}'"))?;
                overrides.sample_rate = Some(parsed.clamp(8_000, 384_000));
            }
            "--buffer-ms" => {
                let value = next_arg(&args, &mut i, "--buffer-ms")?;
                let parsed = value
                    .parse::<u32>()
                    .map_err(|_| anyhow!("flag --buffer-ms: invalid integer '{value}'"))?;
                overrides.buffer_ms = Some(parsed.clamp(20, 2_000));
            }
            "--resample-quality" => {
                let value = next_arg(&args, &mut i, "--resample-quality")?;
                let parsed = value
                    .parse::<u8>()
                    .map_err(|_| anyhow!("flag --resample-quality: invalid integer '{value}'"))?;
                if !(1..=4).contains(&parsed) {
                    return Err(anyhow!("flag --resample-quality must be between 1 and 4"));
                }
                overrides.resample_quality = Some(parsed);
            }
            "--bit-depth" => {
                let value = next_arg(&args, &mut i, "--bit-depth")?;
                let parsed = value
                    .parse::<u16>()
                    .map_err(|_| anyhow!("flag --bit-depth: invalid integer '{value}'"))?;
                if !matches!(parsed, 16 | 32) {
                    return Err(anyhow!("flag --bit-depth must be 16 or 32"));
                }
                overrides.bit_depth = Some(parsed);
            }
            _ => return Err(anyhow!("unknown flag: {arg}")),
        }

        i += 1;
    }

    Ok((CliAction::Run, overrides, positional))
}

fn build_providers(
    cfg: &config::Config,
    provider_name: &str,
) -> Result<(Vec<ProviderEntry>, String, Option<String>)> {
    let normalized = provider_name.trim().to_ascii_lowercase();
    if normalized == "none" {
        return Ok((Vec::new(), "none".to_string(), None));
    }

    if !normalized.is_empty()
        && !matches!(
            normalized.as_str(),
            "radio" | "navidrome" | "youtube" | "yt" | "ytmusic" | "spotify" | "none"
        )
    {
        return Err(anyhow!(
            "unsupported provider '{normalized}' (supported: radio, navidrome, youtube, ytmusic, spotify, none)"
        ));
    }

    #[cfg(not(feature = "spotify-experimental"))]
    if normalized == "spotify" {
        return Err(anyhow!(
            "provider 'spotify' is experimental and not built into this binary. Rebuild with `--features spotify-experimental`. Note: Spotify currently also requires Premium for Web API access."
        ));
    }

    let mut providers = vec![ProviderEntry {
        key: "radio".to_string(),
        name: "Radio".to_string(),
        provider: Box::new(RadioProvider::new()),
    }];
    let mut ytdlp_cookies_from = None;

    if let Some(provider) =
        NavidromeClient::from_config(&cfg.navidrome).or_else(NavidromeClient::from_env)
    {
        providers.push(ProviderEntry {
            key: "navidrome".to_string(),
            name: "Navidrome".to_string(),
            provider: Box::new(provider),
        });
    } else if normalized == "navidrome" {
        return Err(anyhow!(
            "provider 'navidrome' is selected but no Navidrome config/env credentials were found"
        ));
    }

    if let Some(bundle) =
        YtProviderBundle::from_config(&cfg.ytmusic).or_else(YtProviderBundle::from_env)
    {
        ytdlp_cookies_from = bundle.cookies_from();
        providers.push(ProviderEntry {
            key: "ytmusic".to_string(),
            name: "YouTube Music".to_string(),
            provider: Box::new(bundle.music),
        });
        providers.push(ProviderEntry {
            key: "youtube".to_string(),
            name: "YouTube".to_string(),
            provider: Box::new(bundle.video),
        });
    } else if matches!(normalized.as_str(), "youtube" | "yt" | "ytmusic") {
        return Err(anyhow!(
            "provider '{normalized}' is selected but no [ytmusic] client_id/client_secret config (or YTMUSIC_CLIENT_ID / YTMUSIC_CLIENT_SECRET env) was found"
        ));
    }

    #[cfg(feature = "spotify-experimental")]
    {
        if let Some(provider) =
            SpotifyProvider::from_config(&cfg.spotify).or_else(SpotifyProvider::from_env)
        {
            providers.push(ProviderEntry {
                key: "spotify".to_string(),
                name: "Spotify".to_string(),
                provider: Box::new(provider),
            });
        } else if normalized == "spotify" {
            return Err(anyhow!(
                "provider 'spotify' is selected but no [spotify] client_id config (or SPOTIFY_CLIENT_ID env) was found"
            ));
        }
    }

    let default_provider = if normalized.is_empty() {
        if cfg.provider == "yt" {
            "youtube".to_string()
        } else {
            cfg.provider.clone()
        }
    } else if normalized == "yt" {
        "youtube".to_string()
    } else {
        normalized
    };

    Ok((providers, default_provider, ytdlp_cookies_from))
}

fn normalize_search_args(args: Vec<String>) -> Result<Vec<String>> {
    if args.is_empty() {
        return Ok(args);
    }

    match args[0].as_str() {
        "search" => {
            if args.len() == 1 {
                return Err(anyhow!(
                    "search requires a query string (example: rliamp search \"never gonna give you up\")"
                ));
            }
            Ok(vec![format!("ytsearch1:{}", args[1..].join(" "))])
        }
        "search-sc" => {
            if args.len() == 1 {
                return Err(anyhow!(
                    "search-sc requires a query string (example: rliamp search-sc \"lofi\")"
                ));
            }
            Ok(vec![format!("scsearch1:{}", args[1..].join(" "))])
        }
        _ => Ok(args),
    }
}

fn next_arg(args: &[String], index: &mut usize, flag: &str) -> Result<String> {
    if *index + 1 >= args.len() {
        return Err(anyhow!("flag {flag} requires a value"));
    }
    *index += 1;
    Ok(args[*index].clone())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}
