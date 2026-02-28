use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::process::Command;

use crate::playlist::Track;

#[derive(Debug, Deserialize)]
struct FlatEntry {
    url: Option<String>,
    webpage_url: Option<String>,
    title: Option<String>,
    uploader: Option<String>,
    playlist_uploader: Option<String>,
    webpage_url_basename: Option<String>,
}

pub fn resolve_collection(page_url: &str) -> Result<Vec<Track>> {
    ensure_available()?;

    let output = Command::new("yt-dlp")
        .args(["--flat-playlist", "-j", page_url])
        .output()
        .map_err(|err| anyhow!("failed to run yt-dlp: {err}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if err.is_empty() {
            return Err(anyhow!("yt-dlp failed for {page_url}"));
        }
        return Err(anyhow!("yt-dlp: {err}"));
    }

    let mut out = Vec::new();
    let text = String::from_utf8_lossy(&output.stdout);
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(entry) = serde_json::from_str::<FlatEntry>(line) else {
            continue;
        };

        let track_url = entry
            .webpage_url
            .or(entry.url)
            .filter(|v| !v.trim().is_empty());
        let Some(track_url) = track_url else {
            continue;
        };

        let title = entry
            .title
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                entry
                    .webpage_url_basename
                    .map(|v| v.replace('-', " ").trim().to_string())
                    .filter(|v| !v.is_empty())
            })
            .unwrap_or_else(|| track_url.clone());

        let artist = entry
            .uploader
            .or(entry.playlist_uploader)
            .unwrap_or_default();

        out.push(Track {
            path: track_url,
            title,
            artist,
            stream: true,
            ytdlp: true,
        });
    }

    if out.is_empty() {
        out.push(Track {
            path: page_url.to_string(),
            title: page_url.to_string(),
            artist: String::new(),
            stream: true,
            ytdlp: true,
        });
    }

    Ok(out)
}

pub fn resolve_stream_url(page_url: &str) -> Result<String> {
    ensure_available()?;

    let output = Command::new("yt-dlp")
        .args([
            "-f",
            "bestaudio[protocol=https]/bestaudio[protocol=http]/bestaudio",
            "--no-playlist",
            "-g",
            page_url,
        ])
        .output()
        .map_err(|err| anyhow!("failed to run yt-dlp: {err}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if err.is_empty() {
            return Err(anyhow!("yt-dlp failed for {page_url}"));
        }
        return Err(anyhow!("yt-dlp: {err}"));
    }

    for raw in String::from_utf8_lossy(&output.stdout).lines() {
        let line = raw.trim();
        if !line.is_empty() {
            return Ok(line.to_string());
        }
    }

    Err(anyhow!("yt-dlp produced no stream URL for {page_url}"))
}

pub fn ensure_available() -> Result<()> {
    let output = Command::new("yt-dlp")
        .arg("--version")
        .output()
        .map_err(|_| anyhow!("yt-dlp not found in PATH; install it first"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!("yt-dlp not available"))
    }
}
