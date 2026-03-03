use crate::playlist::Track;
use crate::provider::{PlaylistInfo, Provider};
use anyhow::{anyhow, Context, Result};
use rand::RngCore;
use serde::Deserialize;
use std::io::Read;
use std::time::Duration;
use urlencoding::encode;

const NAVIDROME_MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

pub struct NavidromeClient {
    base_url: String,
    user: String,
    password: String,
}

impl NavidromeClient {
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("NAVIDROME_URL").ok()?;
        let user = std::env::var("NAVIDROME_USER").ok()?;
        let password = std::env::var("NAVIDROME_PASS").ok()?;
        let base_url = base_url.trim_end_matches('/').trim();
        if base_url.is_empty() || user.trim().is_empty() || password.trim().is_empty() {
            return None;
        }
        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            return None;
        }
        Some(Self {
            base_url: base_url.to_string(),
            user,
            password,
        })
    }

    fn auth_pairs(&self) -> Vec<(String, String)> {
        let mut salt_bytes = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut salt_bytes);
        let salt = salt_bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let digest = md5::compute(format!("{}{}", self.password, salt));
        let token = format!("{digest:x}");

        vec![
            ("u".to_string(), self.user.clone()),
            ("t".to_string(), token),
            ("s".to_string(), salt),
            ("v".to_string(), "1.0.0".to_string()),
            ("c".to_string(), "rliamp".to_string()),
            ("f".to_string(), "json".to_string()),
        ]
    }

    fn build_url(&self, endpoint: &str, extra: &[(&str, &str)]) -> String {
        let mut pairs = self.auth_pairs();
        for (k, v) in extra {
            pairs.push(((*k).to_string(), (*v).to_string()));
        }

        let query = pairs
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!("{}/rest/{}?{}", self.base_url, endpoint, query)
    }

    fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        extra: &[(&str, &str)],
    ) -> Result<T> {
        let url = self.build_url(endpoint, extra);
        let response = ureq::get(&url)
            .timeout(Duration::from_secs(20))
            .call()
            .map_err(|err| anyhow!("request failed: {err}"))
            .with_context(|| format!("navidrome request: {endpoint}"))?;

        let status = response.status();
        let body = read_body_limited(response, endpoint, NAVIDROME_MAX_BODY_BYTES)
            .with_context(|| format!("reading navidrome response body failed: {endpoint}"))?;

        if status != 200 {
            let snippet = body
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
                .take(180)
                .collect::<String>();
            return Err(anyhow!(
                "navidrome http status {status} on {endpoint}: {snippet}"
            ));
        }

        let value: serde_json::Value =
            serde_json::from_str(&body).map_err(|err| anyhow!("invalid navidrome JSON: {err}"))?;
        check_subsonic_error(&value, endpoint)?;

        serde_json::from_value::<T>(value)
            .map_err(|err| anyhow!("invalid navidrome response payload: {err}"))
    }

    fn stream_url(&self, id: &str) -> String {
        self.build_url("stream", &[("id", id), ("format", "mp3")])
    }
}

fn read_body_limited(response: ureq::Response, endpoint: &str, max_len: usize) -> Result<String> {
    let mut limited = response.into_reader().take((max_len + 1) as u64);
    let mut body = String::new();
    limited
        .read_to_string(&mut body)
        .map_err(|err| anyhow!("failed to read navidrome body for {endpoint}: {err}"))?;
    if body.len() > max_len {
        return Err(anyhow!(
            "navidrome response too large for {endpoint} (>{max_len} bytes)"
        ));
    }
    Ok(body)
}

fn check_subsonic_error(payload: &serde_json::Value, endpoint: &str) -> Result<()> {
    let root = payload
        .get("subsonic-response")
        .ok_or_else(|| anyhow!("missing subsonic-response object in {endpoint}"))?;

    let status = root.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status.is_empty() || status.eq_ignore_ascii_case("ok") {
        return Ok(());
    }

    let err_obj = root.get("error").and_then(|v| v.as_object());
    let code = err_obj
        .and_then(|obj| obj.get("code"))
        .and_then(|v| v.as_i64())
        .unwrap_or_default();
    let message = err_obj
        .and_then(|obj| obj.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown Subsonic error");

    Err(anyhow!(
        "navidrome API error on {endpoint}: status={status}, code={code}, message={message}"
    ))
}

impl Provider for NavidromeClient {
    fn name(&self) -> &str {
        "Navidrome"
    }

    fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
        let parsed: GetPlaylistsResponse = self.get_json("getPlaylists", &[])?;
        let mut out = Vec::new();
        if let Some(items) = parsed.subsonic_response.playlists {
            for item in items.playlist {
                out.push(PlaylistInfo {
                    id: item.id,
                    name: item.name,
                    track_count: item.song_count,
                });
            }
        }
        Ok(out)
    }

    fn tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        let parsed: GetPlaylistResponse = self.get_json("getPlaylist", &[("id", playlist_id)])?;
        let mut out = Vec::new();
        if let Some(pl) = parsed.subsonic_response.playlist {
            for entry in pl.entry {
                out.push(Track {
                    path: self.stream_url(&entry.id),
                    title: entry.title,
                    artist: entry.artist.unwrap_or_default(),
                    stream: true,
                    ytdlp: false,
                });
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Deserialize)]
struct GetPlaylistsResponse {
    #[serde(rename = "subsonic-response")]
    subsonic_response: PlaylistsRoot,
}

#[derive(Debug, Deserialize)]
struct PlaylistsRoot {
    playlists: Option<PlaylistContainer>,
}

#[derive(Debug, Deserialize)]
struct PlaylistContainer {
    playlist: Vec<PlaylistItem>,
}

#[derive(Debug, Deserialize)]
struct PlaylistItem {
    id: String,
    name: String,
    #[serde(rename = "songCount")]
    song_count: usize,
}

#[derive(Debug, Deserialize)]
struct GetPlaylistResponse {
    #[serde(rename = "subsonic-response")]
    subsonic_response: PlaylistRoot,
}

#[derive(Debug, Deserialize)]
struct PlaylistRoot {
    playlist: Option<PlaylistDetail>,
}

#[derive(Debug, Deserialize)]
struct PlaylistDetail {
    entry: Vec<TrackEntry>,
}

#[derive(Debug, Deserialize)]
struct TrackEntry {
    id: String,
    title: String,
    artist: Option<String>,
}
