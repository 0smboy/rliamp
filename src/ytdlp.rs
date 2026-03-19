use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::playlist::Track;

const YTDLP_TIMEOUT: Duration = Duration::from_secs(25);
const YTDLP_WAIT_DELAY: Duration = Duration::from_secs(3);

fn cookies_from_slot() -> &'static Mutex<Option<String>> {
    static SLOT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

pub fn set_cookies_from(browser: Option<String>) {
    let mut guard = cookies_from_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = browser
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
}

fn append_cookie_args(cmd: &mut Command) {
    let browser = cookies_from_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(browser) = browser {
        cmd.args(["--cookies-from-browser", browser.as_str()]);
    }
}

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

    let mut cmd = Command::new("yt-dlp");
    append_cookie_args(&mut cmd);
    let output = command_output_with_timeout(
        cmd.args(["--flat-playlist", "-j", "--socket-timeout", "15", page_url]),
        YTDLP_TIMEOUT,
        "yt-dlp collection probe",
    )?;

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
            realtime: false,
            duration_secs: 0,
        });
    }

    if out.is_empty() {
        out.push(Track {
            path: page_url.to_string(),
            title: page_url.to_string(),
            artist: String::new(),
            stream: true,
            ytdlp: true,
            realtime: false,
            duration_secs: 0,
        });
    }

    Ok(out)
}

pub fn resolve_stream_url(page_url: &str) -> Result<String> {
    ensure_available()?;

    let mut cmd = Command::new("yt-dlp");
    append_cookie_args(&mut cmd);
    let output = command_output_with_timeout(
        cmd.args([
            "-f",
            "bestaudio[protocol=https]/bestaudio[protocol=http]/bestaudio",
            "--no-playlist",
            "--socket-timeout",
            "15",
            "-g",
            page_url,
        ]),
        YTDLP_TIMEOUT,
        "yt-dlp stream resolve",
    )?;

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

fn command_output_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
    label: &str,
) -> Result<Output> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| anyhow!("failed to run yt-dlp: {err}"))?;

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|err| anyhow!("failed waiting for {label}: {err}"))?
            .is_some()
        {
            return child
                .wait_with_output()
                .map_err(|err| anyhow!("failed collecting {label} output: {err}"));
        }

        if started.elapsed() >= timeout {
            let _ = child.kill();
            let deadline = Instant::now() + YTDLP_WAIT_DELAY;
            loop {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                if Instant::now() >= deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
            let _ = child.wait();
            return Err(anyhow!("{label} exceeded timeout of {:?}", timeout));
        }

        thread::sleep(Duration::from_millis(20));
    }
}
