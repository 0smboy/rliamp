use crate::playlist::Track;
use crate::provider::{PlaylistInfo, Provider};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use urlencoding::encode;

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
        if base_url.trim().is_empty() || user.trim().is_empty() || password.trim().is_empty() {
            return None;
        }
        Some(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            user,
            password,
        })
    }

    fn auth_pairs(&self) -> Vec<(String, String)> {
        let salt = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos().to_string())
            .unwrap_or_else(|_| "0".to_string());
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
            .call()
            .map_err(|err| anyhow!("request failed: {err}"))
            .with_context(|| format!("navidrome request: {endpoint}"))?;

        let body = response
            .into_string()
            .map_err(|err| anyhow!("failed to read response body: {err}"))?;

        serde_json::from_str::<T>(&body).map_err(|err| anyhow!("invalid navidrome response: {err}"))
    }

    fn stream_url(&self, id: &str) -> String {
        self.build_url("stream", &[("id", id), ("format", "mp3")])
    }
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
