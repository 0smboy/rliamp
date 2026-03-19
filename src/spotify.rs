use crate::config::SpotifyConfig;
use crate::player::NativeSource;
use crate::playlist::Track;
use crate::provider::{needs_auth, NativeSourceLoader, PlaylistInfo, Provider};
use anyhow::{anyhow, Context, Result};
use librespot_core::{
    authentication::Credentials, config::SessionConfig, session::Session, spotify_uri::SpotifyUri,
};
use librespot_oauth::{OAuthClientBuilder, OAuthToken};
use librespot_playback::{
    audio_backend::{Sink, SinkResult},
    config::PlayerConfig,
    decoder::AudioPacket,
    mixer::NoOpVolume,
    player::{Player as LibrespotPlayer, PlayerEvent},
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:43861/callback";
const BODY_LIMIT: usize = 8 * 1024 * 1024;
const TOKEN_REFRESH_LEEWAY: Duration = Duration::from_secs(60);
const SOURCE_READY_TIMEOUT: Duration = Duration::from_secs(12);
const SPOTIFY_SAMPLE_RATE: f32 = 44_100.0;
const LIKED_TRACKS_ID: &str = "spotify:liked";
const LIKED_TRACKS_NAME: &str = "Liked Songs";
const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "playlist-read-private",
    "playlist-read-collaborative",
    "user-library-read",
];

#[derive(Clone)]
pub struct SpotifyProvider {
    shared: Arc<Mutex<SharedState>>,
}

impl SpotifyProvider {
    pub fn from_config(cfg: &SpotifyConfig) -> Option<Self> {
        Self::from_parts(cfg.client_id.as_deref(), cfg.redirect_uri.as_deref())
    }

    pub fn from_env() -> Option<Self> {
        Self::from_parts(
            std::env::var("SPOTIFY_CLIENT_ID").ok().as_deref(),
            std::env::var("SPOTIFY_REDIRECT_URI").ok().as_deref(),
        )
    }

    fn from_parts(client_id: Option<&str>, redirect_uri: Option<&str>) -> Option<Self> {
        let client_id = client_id?.trim();
        if client_id.is_empty() {
            return None;
        }

        let redirect_uri = redirect_uri
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_REDIRECT_URI);

        Some(Self {
            shared: Arc::new(Mutex::new(SharedState {
                client_id: client_id.to_string(),
                redirect_uri: redirect_uri.to_string(),
                token: None,
                playlists_cache: None,
                track_cache: HashMap::new(),
            })),
        })
    }
}

impl Provider for SpotifyProvider {
    fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
        let mut shared = lock_unpoison(&self.shared);
        if let Some(cached) = shared.playlists_cache.clone() {
            return Ok(cached);
        }
        let playlists = shared.fetch_playlists()?;
        shared.playlists_cache = Some(playlists.clone());
        Ok(playlists)
    }

    fn tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        let mut shared = lock_unpoison(&self.shared);
        if let Some(cached) = shared.track_cache.get(playlist_id).cloned() {
            return Ok(cached);
        }

        let tracks = shared.fetch_tracks(playlist_id)?;
        shared
            .track_cache
            .insert(playlist_id.to_string(), tracks.clone());
        Ok(tracks)
    }

    fn native_loader(
        &self,
        track: &Track,
        output_sample_rate: f32,
    ) -> Result<Option<NativeSourceLoader>> {
        if !track.path.starts_with("spotify:track:") && !track.path.starts_with("spotify:episode:")
        {
            return Ok(None);
        }

        let shared = self.shared.clone();
        let uri = track.path.clone();
        Ok(Some(Box::new(move || {
            let source = SpotifyNativeSource::open(shared, uri, output_sample_rate)?;
            Ok(Box::new(source) as Box<dyn NativeSource>)
        })))
    }

    fn authenticate(&mut self) -> Result<()> {
        let mut shared = lock_unpoison(&self.shared);
        shared.authenticate()?;
        shared.playlists_cache = None;
        shared.track_cache.clear();
        Ok(())
    }

    fn close(&mut self) {
        let mut shared = lock_unpoison(&self.shared);
        shared.token = None;
    }
}

struct SharedState {
    client_id: String,
    redirect_uri: String,
    token: Option<SpotifyToken>,
    playlists_cache: Option<Vec<PlaylistInfo>>,
    track_cache: HashMap<String, Vec<Track>>,
}

impl SharedState {
    fn authenticate(&mut self) -> Result<()> {
        let client = self.oauth_client(true)?;
        let token = client
            .get_access_token()
            .map_err(|err| anyhow!("spotify OAuth failed: {err}"))?;
        self.store_token(token, None)?;
        Ok(())
    }

    fn fetch_playlists(&mut self) -> Result<Vec<PlaylistInfo>> {
        let liked_total: PagedItems<SavedTrackItem> =
            self.get_json("https://api.spotify.com/v1/me/tracks?limit=1")?;
        let mut playlists = vec![PlaylistInfo {
            id: LIKED_TRACKS_ID.to_string(),
            name: LIKED_TRACKS_NAME.to_string(),
            track_count: liked_total.total.unwrap_or_default(),
        }];

        let mut next = Some("https://api.spotify.com/v1/me/playlists?limit=50".to_string());
        while let Some(url) = next {
            let page: PagedItems<WebPlaylist> = self.get_json(&url)?;
            playlists.extend(page.items.into_iter().map(|item| PlaylistInfo {
                id: item.id,
                name: item.name,
                track_count: item.tracks.total,
            }));
            next = page.next;
        }

        Ok(playlists)
    }

    fn fetch_tracks(&mut self, playlist_id: &str) -> Result<Vec<Track>> {
        let mut tracks = Vec::new();
        let mut next = Some(if playlist_id == LIKED_TRACKS_ID {
            "https://api.spotify.com/v1/me/tracks?limit=50".to_string()
        } else {
            format!(
                "https://api.spotify.com/v1/playlists/{}/tracks?limit=100&additional_types=track",
                urlencoding::encode(playlist_id)
            )
        });

        while let Some(url) = next {
            if playlist_id == LIKED_TRACKS_ID {
                let page: PagedItems<SavedTrackItem> = self.get_json(&url)?;
                for item in page.items {
                    if let Some(track) = web_track_to_track(item.track) {
                        tracks.push(track);
                    }
                }
                next = page.next;
            } else {
                let page: PagedItems<PlaylistTrackItem> = self.get_json(&url)?;
                for item in page.items {
                    if let Some(track) = item.track.and_then(web_track_to_track) {
                        tracks.push(track);
                    }
                }
                next = page.next;
            }
        }

        Ok(tracks)
    }

    fn get_json<T: DeserializeOwned>(&mut self, url: &str) -> Result<T> {
        for attempt in 0..2 {
            let access_token = self.ensure_access_token(false)?.access_token.clone();
            let response = match ureq::get(url)
                .set("Authorization", &format!("Bearer {access_token}"))
                .timeout(Duration::from_secs(20))
                .call()
            {
                Ok(response) => response,
                Err(ureq::Error::Status(status, response)) => {
                    let body = read_body_limited(response, BODY_LIMIT).unwrap_or_default();
                    if status == 401 && attempt == 0 {
                        self.token = None;
                        continue;
                    }
                    let snippet = body
                        .chars()
                        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
                        .take(200)
                        .collect::<String>();
                    return Err(anyhow!("spotify http status {status}: {snippet}"));
                }
                Err(err) => {
                    return Err(anyhow!("spotify request failed: {err}"))
                        .with_context(|| format!("spotify request: {url}"));
                }
            };

            let body = read_body_limited(response, BODY_LIMIT)?;
            if body.trim().is_empty() {
                return Err(anyhow!("empty spotify response body"));
            }

            return serde_json::from_str(&body)
                .map_err(|err| anyhow!("invalid spotify JSON payload: {err}"));
        }

        Err(anyhow!("spotify authorization expired"))
    }

    fn ensure_access_token(&mut self, interactive: bool) -> Result<&SpotifyToken> {
        let should_refresh = self
            .token
            .as_ref()
            .map(|token| Instant::now() + TOKEN_REFRESH_LEEWAY >= token.expires_at)
            .unwrap_or(true);
        if !should_refresh {
            return self
                .token
                .as_ref()
                .ok_or_else(|| anyhow!("spotify access token unavailable"));
        }

        let refresh_token = self
            .token
            .as_ref()
            .map(|token| token.refresh_token.clone())
            .or_else(|| load_saved_refresh_token().ok())
            .filter(|value| !value.trim().is_empty());

        if let Some(refresh_token) = refresh_token {
            let client = self.oauth_client(false)?;
            match client.refresh_token(&refresh_token) {
                Ok(token) => {
                    self.store_token(token, Some(refresh_token))?;
                    return self
                        .token
                        .as_ref()
                        .ok_or_else(|| anyhow!("spotify access token unavailable"));
                }
                Err(err) if interactive => {
                    eprintln!("spotify token refresh failed, falling back to re-auth: {err}");
                }
                Err(_) => return Err(needs_auth()),
            }
        } else if !interactive {
            return Err(needs_auth());
        }

        self.authenticate()?;
        self.token
            .as_ref()
            .ok_or_else(|| anyhow!("spotify access token unavailable"))
    }

    fn oauth_client(&self, open_browser: bool) -> Result<librespot_oauth::OAuthClient> {
        let builder =
            OAuthClientBuilder::new(&self.client_id, &self.redirect_uri, OAUTH_SCOPES.to_vec());
        let builder = if open_browser {
            builder.open_in_browser()
        } else {
            builder
        };
        builder
            .build()
            .map_err(|err| anyhow!("spotify OAuth client init failed: {err}"))
    }

    fn store_token(
        &mut self,
        token: OAuthToken,
        fallback_refresh_token: Option<String>,
    ) -> Result<()> {
        let refresh_token = if token.refresh_token.trim().is_empty() {
            fallback_refresh_token.unwrap_or_default()
        } else {
            token.refresh_token.clone()
        };
        if refresh_token.trim().is_empty() {
            return Err(anyhow!("spotify OAuth did not return a refresh token"));
        }

        save_refresh_token(&refresh_token)?;
        self.token = Some(SpotifyToken {
            access_token: token.access_token,
            refresh_token,
            expires_at: token.expires_at,
        });
        Ok(())
    }
}

struct SpotifyToken {
    access_token: String,
    refresh_token: String,
    expires_at: Instant,
}

#[derive(Serialize, Deserialize)]
struct StoredCreds {
    refresh_token: String,
}

fn read_body_limited(response: ureq::Response, max_len: usize) -> Result<String> {
    let mut limited = response.into_reader().take((max_len + 1) as u64);
    let mut body = String::new();
    limited
        .read_to_string(&mut body)
        .context("failed to read spotify response body")?;
    if body.len() > max_len {
        return Err(anyhow!("spotify response exceeded {max_len} bytes"));
    }
    Ok(body)
}

fn config_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    let path = PathBuf::from(home).join(".config").join("rliamp");
    fs::create_dir_all(&path)
        .with_context(|| format!("failed to create config dir {}", path.display()))?;
    Ok(path)
}

fn refresh_token_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("spotify-credentials.json"))
}

fn load_saved_refresh_token() -> Result<String> {
    let path = refresh_token_path()?;
    let body =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let creds: StoredCreds =
        serde_json::from_str(&body).with_context(|| format!("invalid {}", path.display()))?;
    if creds.refresh_token.trim().is_empty() {
        return Err(anyhow!("stored spotify refresh token is empty"));
    }
    Ok(creds.refresh_token)
}

fn save_refresh_token(refresh_token: &str) -> Result<()> {
    let path = refresh_token_path()?;
    let body = serde_json::to_string_pretty(&StoredCreds {
        refresh_token: refresh_token.to_string(),
    })?;
    fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))
}

fn web_track_to_track(track: WebTrack) -> Option<Track> {
    if track.is_local.unwrap_or(false) || track.uri.trim().is_empty() {
        return None;
    }
    if !track.uri.starts_with("spotify:track:") && !track.uri.starts_with("spotify:episode:") {
        return None;
    }

    let artist = if track.artists.is_empty() {
        String::new()
    } else {
        track
            .artists
            .into_iter()
            .map(|artist| artist.name)
            .collect::<Vec<_>>()
            .join(", ")
    };

    Some(Track {
        path: track.uri,
        title: track.name,
        artist,
        stream: true,
        ytdlp: false,
        realtime: false,
        duration_secs: track.duration_ms / 1000,
    })
}

#[derive(Deserialize)]
struct PagedItems<T> {
    items: Vec<T>,
    next: Option<String>,
    total: Option<usize>,
}

#[derive(Deserialize)]
struct WebPlaylist {
    id: String,
    name: String,
    tracks: WebPlaylistTrackCount,
}

#[derive(Deserialize)]
struct WebPlaylistTrackCount {
    total: usize,
}

#[derive(Deserialize)]
struct SavedTrackItem {
    track: WebTrack,
}

#[derive(Deserialize)]
struct PlaylistTrackItem {
    track: Option<WebTrack>,
}

#[derive(Deserialize)]
struct WebTrack {
    uri: String,
    name: String,
    duration_ms: u32,
    #[serde(default)]
    is_local: Option<bool>,
    #[serde(default)]
    artists: Vec<WebArtist>,
}

#[derive(Deserialize)]
struct WebArtist {
    name: String,
}

struct SpotifyNativeSource {
    player: Arc<LibrespotPlayer>,
    _runtime: tokio::runtime::Runtime,
    state: Arc<Mutex<SpotifySourceState>>,
}

impl SpotifyNativeSource {
    fn open(shared: Arc<Mutex<SharedState>>, uri: String, output_sample_rate: f32) -> Result<Self> {
        let spotify_uri = SpotifyUri::from_uri(&uri)
            .map_err(|err| anyhow!("invalid spotify uri '{uri}': {err}"))?;
        let (client_id, access_token) = {
            let mut shared = lock_unpoison(&shared);
            let client_id = shared.client_id.clone();
            let token = shared.ensure_access_token(false)?;
            (client_id, token.access_token.clone())
        };

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("failed to create tokio runtime for spotify")?;

        let mut session_config = SessionConfig::default();
        session_config.client_id = client_id;
        let session = Session::new(session_config, None);
        runtime
            .block_on(session.connect(Credentials::with_access_token(&access_token), false))
            .map_err(|err| anyhow!("spotify session connect failed: {err}"))?;

        let state = Arc::new(Mutex::new(SpotifySourceState::new(output_sample_rate)));
        let sink_state = state.clone();
        let mut player_config = PlayerConfig::default();
        player_config.gapless = false;
        player_config.position_update_interval = Some(Duration::from_millis(250));
        let player =
            LibrespotPlayer::new(player_config, session, Box::new(NoOpVolume), move || {
                Box::new(SpotifyPcmSink::new(sink_state.clone()))
            });

        start_event_bridge(player.clone(), state.clone());
        player.load(spotify_uri, true, 0);

        let started = Instant::now();
        loop {
            let state_guard = lock_unpoison(&state);
            if state_guard.ready || state_guard.failed.is_some() || player.is_invalid() {
                if let Some(err) = state_guard.failed.clone() {
                    return Err(anyhow!(err));
                }
                break;
            }
            if started.elapsed() >= SOURCE_READY_TIMEOUT {
                break;
            }
            drop(state_guard);
            thread::sleep(Duration::from_millis(25));
        }

        Ok(Self {
            player,
            _runtime: runtime,
            state,
        })
    }
}

impl NativeSource for SpotifyNativeSource {
    fn next_stereo(&mut self) -> (f32, f32) {
        let mut state = lock_unpoison(&self.state);
        state.next_stereo()
    }

    fn position(&self) -> Duration {
        lock_unpoison(&self.state).position
    }

    fn duration(&self) -> Duration {
        lock_unpoison(&self.state).duration
    }

    fn seek(&mut self, target: Duration) -> Result<()> {
        {
            let mut state = lock_unpoison(&self.state);
            state.position = target;
            state.finished = false;
            state.failed = None;
            state.clear_frames();
        }
        self.player
            .seek(target.as_millis().min(u128::from(u32::MAX)) as u32);
        Ok(())
    }

    fn play(&mut self) -> Result<()> {
        self.player.play();
        Ok(())
    }

    fn pause(&mut self) -> Result<()> {
        self.player.pause();
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.player.stop();
        Ok(())
    }

    fn is_finished(&self) -> bool {
        let state = lock_unpoison(&self.state);
        state.finished && state.frames.len() <= 1
    }

    fn close(&mut self) {
        let _ = self.stop();
    }
}

fn start_event_bridge(player: Arc<LibrespotPlayer>, state: Arc<Mutex<SpotifySourceState>>) {
    let mut events = player.get_player_event_channel();
    thread::spawn(move || {
        while let Some(event) = events.blocking_recv() {
            let mut state = lock_unpoison(&state);
            match event {
                PlayerEvent::Loading { position_ms, .. }
                | PlayerEvent::Playing { position_ms, .. }
                | PlayerEvent::Paused { position_ms, .. }
                | PlayerEvent::PositionCorrection { position_ms, .. }
                | PlayerEvent::PositionChanged { position_ms, .. }
                | PlayerEvent::Seeked { position_ms, .. } => {
                    state.position = Duration::from_millis(u64::from(position_ms));
                    state.finished = false;
                }
                PlayerEvent::TrackChanged { audio_item } => {
                    state.duration = Duration::from_millis(u64::from(audio_item.duration_ms));
                    state.finished = false;
                }
                PlayerEvent::Unavailable { .. } => {
                    state.failed = Some("spotify track is unavailable".to_string());
                    state.finished = true;
                }
                PlayerEvent::EndOfTrack { .. } | PlayerEvent::Stopped { .. } => {
                    state.finished = true;
                }
                _ => {}
            }
        }
    });
}

struct SpotifyPcmSink {
    state: Arc<Mutex<SpotifySourceState>>,
}

impl SpotifyPcmSink {
    fn new(state: Arc<Mutex<SpotifySourceState>>) -> Self {
        Self { state }
    }
}

impl Sink for SpotifyPcmSink {
    fn write(
        &mut self,
        packet: AudioPacket,
        _converter: &mut librespot_playback::convert::Converter,
    ) -> SinkResult<()> {
        let AudioPacket::Samples(samples) = packet else {
            return Ok(());
        };

        if samples.is_empty() {
            return Ok(());
        }

        let mut state = lock_unpoison(&self.state);
        for chunk in samples.chunks(2) {
            let left = chunk[0] as f32;
            let right = chunk.get(1).copied().unwrap_or(chunk[0]) as f32;
            state.frames.push_back((left, right));
        }
        state.ready = true;
        Ok(())
    }
}

struct SpotifySourceState {
    frames: VecDeque<(f32, f32)>,
    cursor: f64,
    step: f64,
    position: Duration,
    duration: Duration,
    ready: bool,
    finished: bool,
    failed: Option<String>,
}

impl SpotifySourceState {
    fn new(output_sample_rate: f32) -> Self {
        Self {
            frames: VecDeque::new(),
            cursor: 0.0,
            step: SPOTIFY_SAMPLE_RATE as f64 / output_sample_rate.max(1.0) as f64,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            ready: false,
            finished: false,
            failed: None,
        }
    }

    fn clear_frames(&mut self) {
        self.frames.clear();
        self.cursor = 0.0;
        self.ready = false;
    }

    fn next_stereo(&mut self) -> (f32, f32) {
        if self.frames.len() < 2 {
            if self.finished {
                return (0.0, 0.0);
            }
            return (0.0, 0.0);
        }

        let base_idx = self.cursor.floor() as usize;
        let next_idx = base_idx + 1;
        let Some(&(l0, r0)) = self.frames.get(base_idx) else {
            return (0.0, 0.0);
        };
        let Some(&(l1, r1)) = self.frames.get(next_idx) else {
            return (l0, r0);
        };
        let frac = (self.cursor - base_idx as f64) as f32;
        let out = (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac);
        self.cursor += self.step;

        let advance = self.cursor.floor() as usize;
        if advance > 0 {
            let drain = advance.min(self.frames.len().saturating_sub(1));
            for _ in 0..drain {
                self.frames.pop_front();
            }
            self.cursor -= drain as f64;
        }

        out
    }
}

fn lock_unpoison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
