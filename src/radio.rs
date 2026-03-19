use crate::provider::{PlaylistInfo, Provider};
use crate::runtime_url;
use anyhow::{anyhow, Result};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

const BUILTIN_STATIONS: [(&str, &str); 2] = [
    (
        "cliamp radio",
        "https://radio.cliamp.stream/lofi/stream.pls",
    ),
    (
        "cliamp synthwave",
        "https://radio.cliamp.stream/synthwave/stream.pls",
    ),
];

pub struct RadioProvider {
    stations: Vec<Station>,
}

#[derive(Debug, Clone)]
struct Station {
    name: String,
    url: String,
}

impl RadioProvider {
    pub fn new() -> Self {
        let mut stations = BUILTIN_STATIONS
            .iter()
            .map(|(name, url)| Station {
                name: (*name).to_string(),
                url: (*url).to_string(),
            })
            .collect::<Vec<_>>();

        if let Ok(extra) = load_user_stations() {
            stations.extend(extra);
        }

        Self { stations }
    }
}

impl Provider for RadioProvider {
    fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
        Ok(self
            .stations
            .iter()
            .enumerate()
            .map(|(idx, station)| PlaylistInfo {
                id: idx.to_string(),
                name: station.name.clone(),
                track_count: 1,
            })
            .collect())
    }

    fn tracks(&self, playlist_id: &str) -> Result<Vec<crate::playlist::Track>> {
        let idx = playlist_id
            .parse::<usize>()
            .map_err(|_| anyhow!("invalid station id"))?;
        let station = self
            .stations
            .get(idx)
            .ok_or_else(|| anyhow!("station not found"))?;
        let mut tracks = runtime_url::resolve_runtime_url(&station.url)?;
        for track in &mut tracks {
            track.realtime = true;
            track.duration_secs = 0;
        }
        Ok(tracks)
    }
}

fn load_user_stations() -> io::Result<Vec<Station>> {
    let path = radios_config_path()?;
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };

    let mut stations = Vec::new();
    let mut current: Option<Station> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line == "[[station]]" {
            if let Some(station) = current.take() {
                if !station.name.trim().is_empty() && !station.url.trim().is_empty() {
                    stations.push(station);
                }
            }
            current = Some(Station {
                name: String::new(),
                url: String::new(),
            });
            continue;
        }

        let Some(station) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key {
            "name" => station.name = value.to_string(),
            "url" => station.url = value.to_string(),
            _ => {}
        }
    }

    if let Some(station) = current {
        if !station.name.trim().is_empty() && !station.url.trim().is_empty() {
            stations.push(station);
        }
    }

    Ok(stations)
}

fn radios_config_path() -> io::Result<PathBuf> {
    let home = env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME not set"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("rliamp")
        .join("radios.toml"))
}
