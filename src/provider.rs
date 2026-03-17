use crate::playlist::Track;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct PlaylistInfo {
    pub id: String,
    pub name: String,
    pub track_count: usize,
}

pub struct ProviderEntry {
    pub key: String,
    pub name: String,
    pub provider: Box<dyn Provider>,
}

pub trait Provider {
    fn playlists(&self) -> Result<Vec<PlaylistInfo>>;
    fn tracks(&self, playlist_id: &str) -> Result<Vec<Track>>;
}
