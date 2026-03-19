use crate::config::YtMusicConfig;
use crate::playlist::Track;
use crate::provider::{needs_auth, PlaylistInfo, Provider};
use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use urlencoding::decode;

const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
const CALLBACK_PORT: u16 = 19873;
const MUSIC_CATEGORY_ID: &str = "10";

#[derive(Clone)]
pub struct YtProvider {
    shared: Arc<Mutex<SharedState>>,
    kind: YtProviderKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YtProviderKind {
    Music,
    Video,
}

pub struct YtProviderBundle {
    pub music: YtProvider,
    pub video: YtProvider,
}

impl YtProviderBundle {
    pub fn from_config(cfg: &YtMusicConfig) -> Option<Self> {
        Self::from_parts(
            cfg.client_id.as_deref(),
            cfg.client_secret.as_deref(),
            cfg.cookies_from.as_deref(),
        )
    }

    pub fn from_env() -> Option<Self> {
        Self::from_parts(
            std::env::var("YTMUSIC_CLIENT_ID").ok().as_deref(),
            std::env::var("YTMUSIC_CLIENT_SECRET").ok().as_deref(),
            std::env::var("YTMUSIC_COOKIES_FROM").ok().as_deref(),
        )
    }

    fn from_parts(
        client_id: Option<&str>,
        client_secret: Option<&str>,
        cookies_from: Option<&str>,
    ) -> Option<Self> {
        let client_id = client_id?.trim();
        let client_secret = client_secret?.trim();
        if client_id.is_empty() || client_secret.is_empty() {
            return None;
        }

        let shared = Arc::new(Mutex::new(SharedState {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            cookies_from: cookies_from
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            session: None,
            playlists: None,
            classified: HashMap::new(),
            track_cache: HashMap::new(),
        }));

        Some(Self {
            music: YtProvider {
                shared: shared.clone(),
                kind: YtProviderKind::Music,
            },
            video: YtProvider {
                shared,
                kind: YtProviderKind::Video,
            },
        })
    }

    pub fn cookies_from(&self) -> Option<String> {
        self.music
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .cookies_from
            .clone()
    }
}

struct SharedState {
    client_id: String,
    client_secret: String,
    cookies_from: Option<String>,
    session: Option<YtSession>,
    playlists: Option<Vec<PlaylistEntry>>,
    classified: HashMap<String, bool>,
    track_cache: HashMap<String, Vec<Track>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaylistEntry {
    id: String,
    name: String,
    track_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedTrackList {
    fetched_at: u64,
    items: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct YtCache {
    playlists_at: u64,
    playlists: Vec<PlaylistEntry>,
    tracks: HashMap<String, CachedTrackList>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ClassificationCache {
    music: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredCreds {
    refresh_token: String,
}

struct YtSession {
    client_id: String,
    client_secret: String,
    access_token: String,
    refresh_token: String,
}

impl Provider for YtProvider {
    fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
        self.fetch_and_classify()?;
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let playlists = shared.playlists.clone().unwrap_or_default();

        match self.kind {
            YtProviderKind::Music => Ok(playlists
                .into_iter()
                .filter(|playlist| {
                    shared
                        .classified
                        .get(&playlist.id)
                        .copied()
                        .unwrap_or(false)
                })
                .map(to_playlist_info)
                .collect()),
            YtProviderKind::Video => {
                let liked_count = shared.liked_count().unwrap_or(0);
                let mut out = vec![PlaylistInfo {
                    id: "LL".to_string(),
                    name: "Liked Videos".to_string(),
                    track_count: liked_count,
                }];
                out.extend(
                    playlists
                        .into_iter()
                        .filter(|playlist| {
                            !shared
                                .classified
                                .get(&playlist.id)
                                .copied()
                                .unwrap_or(false)
                        })
                        .map(to_playlist_info),
                );
                Ok(out)
            }
        }
    }

    fn tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = shared.track_cache.get(playlist_id).cloned() {
            return Ok(cached);
        }

        if let Some(cached) = load_cache().tracks.get(playlist_id).cloned() {
            if is_fresh(cached.fetched_at) {
                shared
                    .track_cache
                    .insert(playlist_id.to_string(), cached.items.clone());
                return Ok(cached.items);
            }
        }

        let tracks = shared.fetch_tracks(playlist_id)?;
        shared
            .track_cache
            .insert(playlist_id.to_string(), tracks.clone());
        persist_tracks_cache(playlist_id, &tracks);
        Ok(tracks)
    }

    fn authenticate(&mut self) -> Result<()> {
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        shared.ensure_session(true)?;
        shared.playlists = None;
        shared.classified.clear();
        Ok(())
    }

    fn close(&mut self) {
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        shared.session = None;
    }
}

impl SharedState {
    fn ensure_session(&mut self, interactive: bool) -> Result<()> {
        if self.session.is_some() {
            return Ok(());
        }

        let session = if interactive {
            YtSession::authenticate(&self.client_id, &self.client_secret)
        } else {
            YtSession::from_saved(&self.client_id, &self.client_secret).or_else(|err| {
                if err.to_string().contains("stored credentials") {
                    Err(needs_auth())
                } else {
                    Err(err)
                }
            })
        }?;
        self.session = Some(session);
        Ok(())
    }

    fn fetch_and_classify_from_api(&mut self) -> Result<()> {
        self.ensure_session(false)?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow!("ytmusic session unavailable"))?;

        let mut playlists = Vec::new();
        let mut page_token: Option<String> = None;
        let mut seen = HashMap::new();
        loop {
            let mut url = String::from(
                "https://www.googleapis.com/youtube/v3/playlists?part=snippet,contentDetails&mine=true&maxResults=50",
            );
            if let Some(token) = &page_token {
                url.push_str("&pageToken=");
                url.push_str(&urlencoding::encode(token));
            }
            let resp: PlaylistListResponse = session.get_json(&url)?;
            for item in resp.items {
                if item.content_details.item_count == 0 || seen.contains_key(&item.id) {
                    continue;
                }
                seen.insert(item.id.clone(), ());
                playlists.push(PlaylistEntry {
                    id: item.id,
                    name: item.snippet.title,
                    track_count: item.content_details.item_count,
                });
            }
            if resp.next_page_token.is_none() {
                break;
            }
            page_token = resp.next_page_token;
        }

        let classifications = classify_playlists(session, &playlists)?;
        self.playlists = Some(playlists.clone());
        self.classified = classifications.clone();
        persist_playlist_cache(&playlists);
        persist_classification_cache(&classifications);
        Ok(())
    }

    fn fetch_tracks(&mut self, playlist_id: &str) -> Result<Vec<Track>> {
        self.ensure_session(false)?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow!("ytmusic session unavailable"))?;

        let mut tracks = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut url = format!(
                "https://www.googleapis.com/youtube/v3/playlistItems?part=snippet,contentDetails&playlistId={}&maxResults=50",
                urlencoding::encode(playlist_id)
            );
            if let Some(token) = &page_token {
                url.push_str("&pageToken=");
                url.push_str(&urlencoding::encode(token));
            }
            let resp: PlaylistItemsResponse = session.get_json(&url)?;
            for item in resp.items {
                let video_id = item.content_details.video_id.trim().to_string();
                if video_id.is_empty() {
                    continue;
                }
                let title = item.snippet.title.trim().to_string();
                if matches!(title.as_str(), "Private video" | "Deleted video") {
                    continue;
                }
                let channel = item.snippet.video_owner_channel_title.trim().to_string();
                if self.cookies_from.is_none()
                    && (channel.is_empty() || channel == "Music Library Uploads")
                {
                    continue;
                }
                tracks.push(Track {
                    path: format!("https://music.youtube.com/watch?v={video_id}"),
                    title: if title.is_empty() {
                        video_id.clone()
                    } else {
                        title
                    },
                    artist: channel,
                    stream: true,
                    ytdlp: true,
                    realtime: false,
                    duration_secs: 0,
                });
            }
            if resp.next_page_token.is_none() {
                break;
            }
            page_token = resp.next_page_token;
        }

        Ok(tracks)
    }

    fn liked_count(&mut self) -> Result<usize> {
        self.ensure_session(false)?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| anyhow!("ytmusic session unavailable"))?;
        let resp: PlaylistListResponse = session.get_json(
            "https://www.googleapis.com/youtube/v3/playlists?part=contentDetails&id=LL",
        )?;
        Ok(resp
            .items
            .first()
            .map(|item| item.content_details.item_count)
            .unwrap_or(0))
    }
}

impl YtProvider {
    fn fetch_and_classify(&self) -> Result<()> {
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if shared.playlists.is_some() && !shared.classified.is_empty() {
            return Ok(());
        }

        let cache = load_cache();
        let classified = load_classification_cache();
        if is_fresh(cache.playlists_at)
            && !cache.playlists.is_empty()
            && cache
                .playlists
                .iter()
                .all(|playlist| classified.music.contains_key(&playlist.id))
        {
            shared.playlists = Some(cache.playlists);
            shared.classified = classified.music;
            return Ok(());
        }

        shared.fetch_and_classify_from_api()
    }
}

impl YtSession {
    fn from_saved(client_id: &str, client_secret: &str) -> Result<Self> {
        let creds = load_creds().context("missing stored credentials")?;
        let token = refresh_access_token(client_id, client_secret, &creds.refresh_token)?;
        let refresh_token = token
            .refresh_token
            .clone()
            .unwrap_or_else(|| creds.refresh_token.clone());
        save_creds(&StoredCreds {
            refresh_token: refresh_token.clone(),
        })?;
        Ok(Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            access_token: token.access_token,
            refresh_token,
        })
    }

    fn authenticate(client_id: &str, client_secret: &str) -> Result<Self> {
        let verifier = random_verifier();
        let challenge = pkce_challenge(&verifier);
        let redirect_uri = format!("http://127.0.0.1:{CALLBACK_PORT}/callback");
        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256",
            urlencoding::encode(client_id),
            urlencoding::encode(&redirect_uri),
            urlencoding::encode("https://www.googleapis.com/auth/youtube.readonly"),
            urlencoding::encode(&challenge),
        );

        open_browser(&auth_url)?;
        let code = wait_for_oauth_code(&redirect_uri)?;
        let token =
            exchange_code_for_token(client_id, client_secret, &code, &verifier, &redirect_uri)?;
        let refresh_token = token
            .refresh_token
            .clone()
            .ok_or_else(|| anyhow!("google oauth did not return a refresh token"))?;
        save_creds(&StoredCreds {
            refresh_token: refresh_token.clone(),
        })?;

        Ok(Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            access_token: token.access_token,
            refresh_token,
        })
    }

    fn refresh(&mut self) -> Result<()> {
        let token =
            refresh_access_token(&self.client_id, &self.client_secret, &self.refresh_token)?;
        self.access_token = token.access_token;
        if let Some(refresh_token) = token.refresh_token {
            self.refresh_token = refresh_token;
            save_creds(&StoredCreds {
                refresh_token: self.refresh_token.clone(),
            })?;
        }
        Ok(())
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&mut self, url: &str) -> Result<T> {
        match self.get_json_once(url) {
            Ok(value) => Ok(value),
            Err(err) if is_http_status(&err, 401) => {
                self.refresh()?;
                self.get_json_once(url)
            }
            Err(err) => Err(err),
        }
    }

    fn get_json_once<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let response = ureq::get(url)
            .timeout(Duration::from_secs(30))
            .set("Authorization", &format!("Bearer {}", self.access_token))
            .call()
            .map_err(|err| anyhow!("youtube request failed: {err}"))?;
        parse_json_response(response)
    }
}

#[derive(Debug, Deserialize)]
struct PlaylistListResponse {
    #[serde(default)]
    items: Vec<PlaylistItem>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaylistItem {
    id: String,
    #[serde(default)]
    snippet: PlaylistSnippet,
    #[serde(rename = "contentDetails", default)]
    content_details: PlaylistContentDetails,
}

#[derive(Debug, Deserialize, Default)]
struct PlaylistSnippet {
    #[serde(default)]
    title: String,
}

#[derive(Debug, Deserialize, Default)]
struct PlaylistContentDetails {
    #[serde(rename = "itemCount", default)]
    item_count: usize,
}

#[derive(Debug, Deserialize)]
struct PlaylistItemsResponse {
    #[serde(default)]
    items: Vec<PlaylistTrackItem>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct PlaylistTrackItem {
    #[serde(default)]
    snippet: PlaylistTrackSnippet,
    #[serde(rename = "contentDetails", default)]
    content_details: PlaylistTrackContentDetails,
}

#[derive(Debug, Deserialize, Default)]
struct PlaylistTrackSnippet {
    #[serde(default)]
    title: String,
    #[serde(rename = "videoOwnerChannelTitle", default)]
    video_owner_channel_title: String,
}

#[derive(Debug, Deserialize, Default)]
struct PlaylistTrackContentDetails {
    #[serde(rename = "videoId", default)]
    video_id: String,
}

#[derive(Debug, Deserialize)]
struct VideosResponse {
    #[serde(default)]
    items: Vec<VideoItem>,
}

#[derive(Debug, Deserialize)]
struct VideoItem {
    id: String,
    #[serde(default)]
    snippet: VideoSnippet,
}

#[derive(Debug, Deserialize, Default)]
struct VideoSnippet {
    #[serde(rename = "categoryId", default)]
    category_id: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

fn classify_playlists(
    session: &mut YtSession,
    playlists: &[PlaylistEntry],
) -> Result<HashMap<String, bool>> {
    let mut classification = load_classification_cache().music;
    let mut sample_video_ids = Vec::new();
    let mut playlist_by_video = HashMap::new();

    for playlist in playlists {
        if classification.contains_key(&playlist.id) {
            continue;
        }

        let url = format!(
            "https://www.googleapis.com/youtube/v3/playlistItems?part=contentDetails&playlistId={}&maxResults=1",
            urlencoding::encode(&playlist.id)
        );
        let response: PlaylistItemsResponse = session.get_json(&url)?;
        let video_id = response
            .items
            .first()
            .map(|item| item.content_details.video_id.trim().to_string())
            .filter(|video_id| !video_id.is_empty());
        if let Some(video_id) = video_id {
            playlist_by_video.insert(video_id.clone(), playlist.id.clone());
            sample_video_ids.push(video_id);
        } else {
            classification.insert(playlist.id.clone(), false);
        }
    }

    for chunk in sample_video_ids.chunks(50) {
        let joined = chunk.join(",");
        let url = format!(
            "https://www.googleapis.com/youtube/v3/videos?part=snippet&id={}",
            urlencoding::encode(&joined)
        );
        let response: VideosResponse = session.get_json(&url)?;
        for item in response.items {
            if let Some(playlist_id) = playlist_by_video.get(&item.id) {
                classification.insert(
                    playlist_id.clone(),
                    item.snippet.category_id == MUSIC_CATEGORY_ID,
                );
            }
        }
    }

    for playlist in playlists {
        classification.entry(playlist.id.clone()).or_insert(false);
    }

    Ok(classification)
}

fn random_verifier() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(96)
        .map(char::from)
        .collect()
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn wait_for_oauth_code(redirect_uri: &str) -> Result<String> {
    let listener = TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .with_context(|| format!("listen on port {CALLBACK_PORT}"))?;
    let (mut stream, _) = listener.accept().context("waiting for oauth callback")?;

    let mut request = [0u8; 4096];
    let read = stream
        .read(&mut request)
        .context("reading oauth callback")?;
    let line = String::from_utf8_lossy(&request[..read]);
    let first_line = line.lines().next().unwrap_or_default();
    let code = extract_code_from_request(first_line)
        .ok_or_else(|| anyhow!("oauth callback missing code parameter"))?;

    let body = "<html><body style=\"font-family:system-ui;background:#1f2430;color:#d8dee9;display:flex;align-items:center;justify-content:center;height:100vh\"><div><h2>Authenticated</h2><p>You can close this tab now.</p></div></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    if !redirect_uri.contains(&CALLBACK_PORT.to_string()) {
        return Err(anyhow!("unexpected redirect URI"));
    }
    Ok(code)
}

fn extract_code_from_request(request_line: &str) -> Option<String> {
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == "code" {
            return decode(value).ok().map(|value| value.into_owned());
        }
    }
    None
}

fn exchange_code_for_token(
    client_id: &str,
    client_secret: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse> {
    let response = ureq::post("https://oauth2.googleapis.com/token")
        .send_form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .map_err(|err| anyhow!("google token exchange failed: {err}"))?;
    parse_json_response(response)
}

fn refresh_access_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TokenResponse> {
    let response = ureq::post("https://oauth2.googleapis.com/token")
        .send_form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .map_err(|err| anyhow!("google token refresh failed: {err}"))?;
    parse_json_response(response)
}

fn persist_playlist_cache(playlists: &[PlaylistEntry]) {
    let mut cache = load_cache();
    cache.playlists_at = now_ts();
    cache.playlists = playlists.to_vec();
    save_cache(&cache);
}

fn persist_tracks_cache(playlist_id: &str, tracks: &[Track]) {
    let mut cache = load_cache();
    cache.tracks.insert(
        playlist_id.to_string(),
        CachedTrackList {
            fetched_at: now_ts(),
            items: tracks.to_vec(),
        },
    );
    save_cache(&cache);
}

fn persist_classification_cache(classified: &HashMap<String, bool>) {
    let cache = ClassificationCache {
        music: classified.clone(),
    };
    let _ = write_json_file(classification_path(), &cache);
}

fn load_cache() -> YtCache {
    read_json_file(cache_path()).unwrap_or_default()
}

fn save_cache(cache: &YtCache) {
    let _ = write_json_file(cache_path(), cache);
}

fn load_classification_cache() -> ClassificationCache {
    read_json_file(classification_path()).unwrap_or_default()
}

fn load_creds() -> Result<StoredCreds> {
    read_json_file(credentials_path()).context("stored credentials not found")
}

fn save_creds(creds: &StoredCreds) -> Result<()> {
    write_json_file(credentials_path(), creds)
}

fn credentials_path() -> PathBuf {
    app_dir().join("ytmusic_credentials.json")
}

fn cache_path() -> PathBuf {
    app_dir().join("ytmusic_cache.json")
}

fn classification_path() -> PathBuf {
    app_dir().join("ytmusic_classification.json")
}

fn app_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("rliamp")
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: PathBuf) -> Result<T> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|err| anyhow!("invalid json cache: {err}"))
}

fn write_json_file<T: Serialize>(path: PathBuf, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn is_fresh(fetched_at: u64) -> bool {
    let age = now_ts().saturating_sub(fetched_at);
    age < CACHE_TTL_SECS
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn to_playlist_info(entry: PlaylistEntry) -> PlaylistInfo {
    PlaylistInfo {
        id: entry.id,
        name: entry.name,
        track_count: entry.track_count,
    }
}

fn open_browser(url: &str) -> Result<()> {
    let candidates = if cfg!(target_os = "macos") {
        vec![("open", vec![url])]
    } else if cfg!(target_os = "windows") {
        vec![("cmd", vec!["/C", "start", "", url])]
    } else {
        vec![("xdg-open", vec![url])]
    };

    for (program, args) in candidates {
        if Command::new(program).args(args).spawn().is_ok() {
            return Ok(());
        }
    }

    Err(anyhow!("failed to open browser for Google OAuth"))
}

fn parse_json_response<T: for<'de> Deserialize<'de>>(response: ureq::Response) -> Result<T> {
    serde_json::from_reader(response.into_reader())
        .map_err(|err| anyhow!("invalid json response: {err}"))
}

fn is_http_status(err: &anyhow::Error, status: u16) -> bool {
    err.to_string().contains(&format!("status code {status}"))
        || err.to_string().contains(&format!("status: {status}"))
        || err.to_string().contains(&format!("{status}"))
}
