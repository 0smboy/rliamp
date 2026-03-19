use crate::player::NativeSource;
use crate::playlist::Track;
use anyhow::{anyhow, Error, Result};
use std::fmt;

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

#[derive(Debug)]
pub struct NeedsAuthError;

pub type NativeSourceLoader = Box<dyn FnOnce() -> Result<Box<dyn NativeSource>> + Send>;

impl fmt::Display for NeedsAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "provider authentication required")
    }
}

impl std::error::Error for NeedsAuthError {}

pub fn needs_auth() -> Error {
    anyhow!(NeedsAuthError)
}

pub fn is_needs_auth(err: &Error) -> bool {
    err.downcast_ref::<NeedsAuthError>().is_some()
}

pub trait Provider {
    fn playlists(&self) -> Result<Vec<PlaylistInfo>>;
    fn tracks(&self, playlist_id: &str) -> Result<Vec<Track>>;
    fn native_loader(
        &self,
        _track: &Track,
        _output_sample_rate: f32,
    ) -> Result<Option<NativeSourceLoader>> {
        Ok(None)
    }
    fn authenticate(&mut self) -> Result<()> {
        Err(anyhow!(
            "interactive auth is not supported for this provider"
        ))
    }
    fn close(&mut self) {}
}
