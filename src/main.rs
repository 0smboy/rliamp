mod background;
mod config;
mod navidrome;
mod player;
mod playlist;
mod provider;
mod ui;
mod visualizer;
mod ytdlp;

use anyhow::{anyhow, Context, Result};
use glob::glob;
use navidrome::NavidromeClient;
use playlist::{is_feed, is_m3u, is_url, is_ytdl, Playlist, Track};
use provider::Provider;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let provider = NavidromeClient::from_env().map(|p| Box::new(p) as Box<dyn Provider>);

    if args.is_empty() && provider.is_none() {
        return Err(anyhow!(
            "usage: rliamp <file|folder|url> [...] or configure a provider via ENV\n\nexamples:\n  rliamp song.mp3\n  rliamp ~/Music\n  rliamp https://example.com/stream.m3u\n  rliamp https://soundcloud.com/user/sets/playlist\n\nprovider env (Navidrome):\n  NAVIDROME_URL NAVIDROME_USER NAVIDROME_PASS\n\noptional tools:\n  yt-dlp (for SoundCloud/YouTube/Bandcamp URLs)"
        ));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    let mut url_tracks: Vec<String> = Vec::new();
    let mut feed_tracks: Vec<Track> = Vec::new();

    for arg in args {
        if is_url(&arg) {
            if is_feed(&arg) {
                feed_tracks
                    .extend(resolve_feed(&arg).with_context(|| format!("resolving feed: {arg}"))?);
            } else if is_m3u(&arg) {
                let streams = resolve_m3u(&arg).with_context(|| format!("resolving m3u: {arg}"))?;
                url_tracks.extend(streams);
            } else if is_ytdl(&arg) {
                feed_tracks.extend(
                    ytdlp::resolve_collection(&arg)
                        .with_context(|| format!("resolving yt-dlp collection: {arg}"))?,
                );
            } else {
                url_tracks.push(arg);
            }
            continue;
        }

        match glob(&arg) {
            Ok(paths) => {
                let mut matched = false;
                for entry in paths.flatten() {
                    matched = true;
                    collect_audio_files(&entry, &mut files)?;
                }
                if !matched {
                    collect_audio_files(Path::new(&arg), &mut files)?;
                }
            }
            Err(_) => collect_audio_files(Path::new(&arg), &mut files)?,
        }
    }

    files.sort();
    files.dedup();
    url_tracks.sort();
    url_tracks.dedup();

    let mut playlist = Playlist::new();
    playlist.add(
        files
            .into_iter()
            .map(|path| Track::from_path(path.to_string_lossy().to_string())),
    );
    playlist.add(url_tracks.into_iter().map(Track::from_path));
    playlist.add(feed_tracks);

    if playlist.len() == 0 && provider.is_none() {
        return Err(anyhow!("no playable files found"));
    }

    let cfg = config::Config::load().unwrap_or_default();

    match cfg.repeat.as_str() {
        "all" => playlist.cycle_repeat(),
        "one" => {
            playlist.cycle_repeat();
            playlist.cycle_repeat();
        }
        _ => {}
    }
    if cfg.shuffle {
        playlist.toggle_shuffle();
    }

    let player = player::Player::new()?;
    player.set_volume(cfg.volume);
    if cfg.mono {
        player.toggle_mono();
    }
    if cfg.eq_preset.is_empty() || cfg.eq_preset.eq_ignore_ascii_case("custom") {
        for (i, gain) in cfg.eq.into_iter().enumerate() {
            player.set_eq_band(i, gain);
        }
    }

    let mut app = ui::App::new(player, playlist, provider);
    if !cfg.eq_preset.is_empty() && !cfg.eq_preset.eq_ignore_ascii_case("custom") {
        app.set_eq_preset_by_name(&cfg.eq_preset);
    }
    app.run()
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

fn resolve_m3u(url: &str) -> Result<Vec<String>> {
    let response = ureq::get(url)
        .call()
        .map_err(|err| anyhow!("request failed: {err}"))?;
    let body = response
        .into_string()
        .map_err(|err| anyhow!("failed to read m3u body: {err}"))?;

    let mut urls = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        urls.push(line.to_string());
    }

    Ok(urls)
}

fn resolve_feed(url: &str) -> Result<Vec<Track>> {
    let response = ureq::get(url)
        .call()
        .map_err(|err| anyhow!("request failed: {err}"))?;
    let xml = response
        .into_string()
        .map_err(|err| anyhow!("failed to read feed body: {err}"))?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut in_item = false;
    let mut current_tag: Vec<u8> = Vec::new();

    let mut channel_title = String::new();
    let mut item_title = String::new();
    let mut enclosure_url = String::new();
    let mut tracks = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_vec();
                if name.as_slice() == b"item" {
                    in_item = true;
                    item_title.clear();
                    enclosure_url.clear();
                }
                current_tag = name;
            }
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"enclosure" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"url" {
                            enclosure_url =
                                String::from_utf8_lossy(attr.value.as_ref()).to_string();
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let raw = String::from_utf8_lossy(t.as_ref());
                let text = quick_xml::escape::unescape(&raw)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| raw.into_owned());

                if current_tag.as_slice() == b"title" {
                    if in_item {
                        item_title = text;
                    } else if channel_title.is_empty() {
                        channel_title = text;
                    }
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"item" {
                    if !enclosure_url.is_empty() {
                        tracks.push(Track {
                            path: enclosure_url.clone(),
                            title: if item_title.is_empty() {
                                "Untitled Episode".to_string()
                            } else {
                                item_title.clone()
                            },
                            artist: channel_title.clone(),
                            stream: true,
                            ytdlp: false,
                        });
                    }
                    in_item = false;
                    item_title.clear();
                    enclosure_url.clear();
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(anyhow!("xml parse error: {err}")),
            _ => {}
        }
        buf.clear();
    }

    Ok(tracks)
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}
