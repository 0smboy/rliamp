mod background;
mod config;
mod lyrics;
mod navidrome;
mod player;
mod playlist;
mod provider;
mod radio;
mod runtime_url;
mod ui;
mod visualizer;
mod ytdlp;

use anyhow::{anyhow, Context, Result};
use glob::glob;
use navidrome::NavidromeClient;
use playlist::{is_url, Playlist, Track};
use provider::Provider;
use radio::RadioProvider;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug, Clone, Default)]
struct CliOverrides {
    volume: Option<f32>,
    shuffle: Option<bool>,
    repeat: Option<String>,
    mono: Option<bool>,
    theme: Option<String>,
    provider: Option<String>,
    eq_preset: Option<String>,
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
  --provider <name>      Provider: radio or navidrome
  --eq-preset <name>     EQ preset name (e.g. Bass Boost)
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
  (env fallback when [navidrome] is not set in ~/.config/rliamp/config.toml)

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
    let provider = build_provider(&cfg, &provider_name)?;

    if args.is_empty() && provider.is_none() {
        return Err(anyhow!(
            "usage: rliamp <file|folder|url> [...] or configure Navidrome via ~/.config/rliamp/config.toml [navidrome] (or env fallback)\n\nexamples:\n  rliamp song.mp3\n  rliamp ~/Music\n  rliamp ~/radio-stations.m3u\n  rliamp ~/radio-stations.pls\n  rliamp https://example.com/stream.m3u\n  rliamp https://soundcloud.com/user/sets/playlist\n\nprovider config section:\n  [navidrome]\n  url = \"https://navidrome.example.com\"\n  user = \"alice\"\n  password = \"secret\"\n\nprovider env fallback:\n  NAVIDROME_URL NAVIDROME_USER NAVIDROME_PASS (or NAVIDROME_TOKEN)\n\noptional tools:\n  yt-dlp (for SoundCloud/YouTube/Bandcamp URLs)"
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

    if playlist.len() == 0 && provider.is_none() {
        return Err(anyhow!("no playable files found"));
    }

    let volume = overrides.volume.unwrap_or(cfg.volume);
    let repeat = overrides.repeat.unwrap_or(cfg.repeat);
    let shuffle = overrides.shuffle.unwrap_or(cfg.shuffle);
    let mono = overrides.mono.unwrap_or(cfg.mono);
    let eq_preset = overrides.eq_preset.unwrap_or(cfg.eq_preset);
    let theme = overrides.theme.or(cfg.theme);
    let visualizer = cfg.visualizer.clone();

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

    let player = player::Player::new()?;
    player.set_volume(volume);
    if mono {
        player.toggle_mono();
    }
    if eq_preset.is_empty() || eq_preset.eq_ignore_ascii_case("custom") {
        for (i, gain) in cfg.eq.into_iter().enumerate() {
            player.set_eq_band(i, gain);
        }
    }

    let mut app = ui::App::new(player, playlist, provider);
    app.set_seek_large_step_sec(cfg.seek_large_step_sec);
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
    app.set_auto_play(overrides.auto_play);
    app.run()
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
            "--provider" => overrides.provider = Some(next_arg(&args, &mut i, "--provider")?),
            "--eq-preset" => overrides.eq_preset = Some(next_arg(&args, &mut i, "--eq-preset")?),
            _ => return Err(anyhow!("unknown flag: {arg}")),
        }

        i += 1;
    }

    Ok((CliAction::Run, overrides, positional))
}

fn build_provider(cfg: &config::Config, provider_name: &str) -> Result<Option<Box<dyn Provider>>> {
    let normalized = provider_name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "none" => Ok(None),
        "radio" => Ok(Some(Box::new(RadioProvider::new()))),
        "navidrome" => NavidromeClient::from_config(&cfg.navidrome)
            .or_else(NavidromeClient::from_env)
            .map(|provider| Box::new(provider) as Box<dyn Provider>)
            .ok_or_else(|| {
                anyhow!(
                    "provider 'navidrome' is selected but no Navidrome config/env credentials were found"
                )
            })
            .map(Some),
        other => Err(anyhow!(
            "unsupported provider '{other}' (supported: radio, navidrome)"
        )),
    }
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

fn resolve_local_input(
    path: &Path,
    files: &mut Vec<PathBuf>,
    tracks: &mut Vec<Track>,
) -> Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };

    if meta.is_file() {
        if is_local_m3u(path) {
            tracks.extend(resolve_local_m3u(path)?);
            return Ok(());
        }
        if is_local_pls(path) {
            tracks.extend(resolve_local_pls(path)?);
            return Ok(());
        }
        if player::is_supported_path(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if meta.is_dir() {
        collect_audio_files(path, files)?;
    }

    Ok(())
}

fn is_local_m3u(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(ext.to_ascii_lowercase().as_str(), "m3u" | "m3u8")
}

fn is_local_pls(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    ext.eq_ignore_ascii_case("pls")
}

fn collect_audio_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };

    if meta.is_file() {
        if player::is_supported_path(path) {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }

    if !meta.is_dir() {
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(path)?
        .filter_map(|e| e.ok().map(|it| it.path()))
        .collect();
    entries.sort();

    for p in entries {
        collect_audio_files(&p, out)?;
    }

    Ok(())
}

fn resolve_local_m3u(path: &Path) -> Result<Vec<Track>> {
    let body =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(parse_m3u_tracks(&body, path.parent()))
}

fn parse_m3u_tracks(body: &str, base_dir: Option<&Path>) -> Vec<Track> {
    let mut tracks = Vec::new();
    let mut pending_title: Option<String> = None;

    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let title = rest
                .split_once(',')
                .map(|(_, t)| t.trim())
                .unwrap_or_default()
                .to_string();
            if !title.is_empty() {
                pending_title = Some(title);
            }
            continue;
        }

        if line.starts_with('#') {
            continue;
        }

        let mut track = if is_url(line) {
            Track::from_path(line.to_string())
        } else {
            // Remote M3U entries should never resolve to local filesystem paths.
            if base_dir.is_none() {
                continue;
            }

            let Some(resolved) = resolve_local_playlist_path(base_dir, line) else {
                continue;
            };
            Track::from_path(resolved.to_string_lossy().to_string())
        };

        if let Some(title) = pending_title.take() {
            apply_title_hint(&mut track, title);
        }
        tracks.push(track);
    }

    tracks
}

fn resolve_local_pls(path: &Path) -> Result<Vec<Track>> {
    let body =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_pls_tracks(&body, path.parent())
}

fn parse_pls_tracks(body: &str, base_dir: Option<&Path>) -> Result<Vec<Track>> {
    let mut files = BTreeMap::<usize, String>::new();
    let mut titles = BTreeMap::<usize, String>::new();

    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('[')
            || line.starts_with(';')
            || line.starts_with('#')
        {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        let lower = key.to_ascii_lowercase();
        if let Some(num) = lower
            .strip_prefix("file")
            .and_then(|s| s.parse::<usize>().ok())
        {
            files.insert(num, value.to_string());
            continue;
        }
        if let Some(num) = lower
            .strip_prefix("title")
            .and_then(|s| s.parse::<usize>().ok())
        {
            titles.insert(num, value.to_string());
        }
    }

    if files.is_empty() {
        return Err(anyhow!("no entries found in PLS playlist"));
    }

    let all_streams = files.len() > 1 && files.values().all(|p| is_url(p));
    if all_streams {
        let (&first_idx, first_path) = files
            .iter()
            .next()
            .ok_or_else(|| anyhow!("no entries found in PLS playlist"))?;
        let mut track = Track::from_path(first_path.to_string());
        if let Some(title) = titles.get(&first_idx) {
            let cleaned = strip_mirror_suffix(title.trim());
            if !cleaned.is_empty() {
                apply_title_hint(&mut track, cleaned.to_string());
            }
        }
        return Ok(vec![track]);
    }

    let mut out = Vec::new();
    for (idx, raw_path) in files {
        let mut track = if is_url(&raw_path) {
            Track::from_path(raw_path)
        } else {
            // Remote PLS entries should never resolve to local filesystem paths.
            if base_dir.is_none() {
                continue;
            }

            let Some(resolved) = resolve_local_playlist_path(base_dir, raw_path.as_str()) else {
                continue;
            };
            Track::from_path(resolved.to_string_lossy().to_string())
        };
        if let Some(title) = titles.get(&idx) {
            apply_title_hint(&mut track, title.trim().to_string());
        }
        out.push(track);
    }
    Ok(out)
}

fn apply_title_hint(track: &mut Track, title: String) {
    if let Some((artist, song)) = title.split_once(" - ") {
        if track.artist.is_empty() {
            track.artist = artist.trim().to_string();
        }
        track.title = song.trim().to_string();
    } else if !title.trim().is_empty() {
        track.title = title.trim().to_string();
    }
}

fn strip_mirror_suffix(s: &str) -> &str {
    if let Some(i) = s.rfind("(#") {
        if s.ends_with(')') {
            return s[..i].trim_end_matches([' ', ':']).trim();
        }
    }
    s
}

fn resolve_local_playlist_path(base_dir: Option<&Path>, raw: &str) -> Option<PathBuf> {
    if raw.is_empty() || raw.contains('\0') {
        return None;
    }

    let p = Path::new(raw);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }

    let Some(base) = base_dir else {
        return Some(p.to_path_buf());
    };
    let normalized_base = normalize_path_lexical(base);
    let normalized_target = normalize_path_lexical(base.join(p));

    if !normalized_target.starts_with(&normalized_base) {
        return None;
    }
    Some(normalized_target)
}

fn normalize_path_lexical(path: impl AsRef<Path>) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.as_ref().components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}
