use encoding_rs::{WINDOWS_1251, WINDOWS_1253, WINDOWS_1255, WINDOWS_1256, WINDOWS_874};
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTagKey, Tag};
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;
use urlencoding::decode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn cycle(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }
}

impl Default for RepeatMode {
    fn default() -> Self {
        RepeatMode::Off
    }
}

impl std::fmt::Display for RepeatMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            RepeatMode::Off => "Off",
            RepeatMode::All => "All",
            RepeatMode::One => "One",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub stream: bool,
    pub ytdlp: bool,
    pub realtime: bool,
    pub duration_secs: u32,
}

impl Track {
    pub fn from_path(path: impl Into<String>) -> Self {
        let path = path.into();
        if is_url(&path) {
            return track_from_url(path);
        }

        if let Some(track) = track_from_embedded_tags(&path) {
            return track;
        }

        track_from_filename(path)
    }

    pub fn display_name(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} - {}", self.artist, self.title)
        }
    }

    pub fn is_live(&self) -> bool {
        self.realtime
    }
}

fn track_from_embedded_tags(path: &str) -> Option<Track> {
    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }

    let mut probed = get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let mut title: Option<String> = None;
    let mut artist: Option<String> = None;

    if let Some(mut metadata) = probed.metadata.get() {
        extract_tags_from_log(&mut metadata, &mut title, &mut artist);
    }
    {
        let mut metadata = probed.format.metadata();
        extract_tags_from_log(&mut metadata, &mut title, &mut artist);
    }

    if title.is_none() && artist.is_none() {
        return None;
    }

    let mut track = track_from_filename(path.to_string());
    if let Some(value) = title {
        track.title = value;
    }
    if let Some(value) = artist {
        track.artist = value;
    }
    Some(track)
}

fn track_from_filename(path: String) -> Track {
    let base = Path::new(&path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path.as_str())
        .to_string();

    let name = Path::new(&base)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(base.as_str())
        .to_string();

    if let Some((artist, title)) = name.as_str().split_once(" - ") {
        return Track {
            path,
            title: title.trim().to_string(),
            artist: artist.trim().to_string(),
            stream: false,
            ytdlp: false,
            realtime: false,
            duration_secs: 0,
        };
    }

    Track {
        path,
        title: name,
        artist: String::new(),
        stream: false,
        ytdlp: false,
        realtime: false,
        duration_secs: 0,
    }
}

fn extract_tags_from_log(
    metadata: &mut symphonia::core::meta::Metadata<'_>,
    title: &mut Option<String>,
    artist: &mut Option<String>,
) {
    loop {
        if let Some(rev) = metadata.current() {
            extract_tags(rev.tags(), title, artist);
            if title.is_some() && artist.is_some() {
                return;
            }
        }
        if metadata.pop().is_none() {
            return;
        }
    }
}

fn extract_tags(tags: &[Tag], title: &mut Option<String>, artist: &mut Option<String>) {
    for tag in tags {
        let value = sanitize_tag(tag.value.to_string());
        if value.is_empty() {
            continue;
        }

        match tag.std_key {
            Some(StandardTagKey::TrackTitle) | Some(StandardTagKey::SortTrackTitle) => {
                assign_if_empty(title, value.clone());
            }
            Some(StandardTagKey::Artist)
            | Some(StandardTagKey::AlbumArtist)
            | Some(StandardTagKey::OriginalArtist)
            | Some(StandardTagKey::Performer) => {
                assign_if_empty(artist, value.clone());
            }
            _ => {}
        }

        let key = tag.key.to_ascii_lowercase();
        if key == "title" || key == "tracktitle" {
            assign_if_empty(title, value.clone());
        } else if key == "artist" || key == "albumartist" || key == "performer" || key == "author" {
            assign_if_empty(artist, value.clone());
        }
    }
}

fn assign_if_empty(target: &mut Option<String>, value: String) {
    if target
        .as_ref()
        .map(|current| current.trim().is_empty())
        .unwrap_or(true)
    {
        *target = Some(value);
    }
}

fn sanitize_tag(raw: String) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut total = 0usize;
    let mut high = 0usize;
    for ch in trimmed.chars() {
        total += 1;
        if ('\u{0080}'..='\u{00FF}').contains(&ch) {
            high += 1;
        }
    }
    if total == 0 || high * 3 < total {
        return trimmed.to_string();
    }

    let mut raw_bytes = Vec::with_capacity(total);
    for ch in trimmed.chars() {
        if (ch as u32) > 0xFF {
            return trimmed.to_string();
        }
        raw_bytes.push(ch as u8);
    }

    if let Ok(decoded_utf8) = String::from_utf8(raw_bytes.clone()) {
        if !decoded_utf8.trim().is_empty() {
            return decoded_utf8.trim().to_string();
        }
    }

    let encodings = [
        WINDOWS_1255,
        WINDOWS_1256,
        WINDOWS_1251,
        WINDOWS_1253,
        WINDOWS_874,
    ];

    let mut best_text = String::new();
    let mut best_score = 0usize;
    for encoding in encodings {
        let (decoded, _, had_errors) = encoding.decode(&raw_bytes);
        if had_errors {
            continue;
        }
        let candidate = decoded.trim();
        if candidate.is_empty() {
            continue;
        }

        let score = candidate
            .chars()
            .filter(|ch| ch.is_alphabetic() && *ch > '\u{024F}')
            .count();
        if score > best_score {
            best_score = score;
            best_text = candidate.to_string();
        }
    }

    if !best_text.is_empty() {
        best_text
    } else {
        trimmed.to_string()
    }
}

pub fn is_url(path: &str) -> bool {
    path.starts_with("http://")
        || path.starts_with("https://")
        || path.starts_with("ytsearch:")
        || path.starts_with("ytsearch1:")
        || path.starts_with("scsearch:")
        || path.starts_with("scsearch1:")
}

pub fn is_ytdl(path: &str) -> bool {
    if !is_url(path) {
        return false;
    }

    if path.starts_with("ytsearch:")
        || path.starts_with("ytsearch1:")
        || path.starts_with("scsearch:")
        || path.starts_with("scsearch1:")
    {
        return true;
    }

    let Some(host) = normalized_url_host(path) else {
        return false;
    };

    matches!(
        host.as_str(),
        "soundcloud.com"
            | "youtube.com"
            | "youtu.be"
            | "music.youtube.com"
            | "bandcamp.com"
            | "music.163.com"
            | "bilibili.com"
            | "b23.tv"
    ) || host.ends_with(".bandcamp.com")
        || host.ends_with(".bilibili.com")
}

pub fn is_xiaoyuzhou_episode(path: &str) -> bool {
    if !is_url(path) {
        return false;
    }

    let Some(host) = normalized_url_host(path) else {
        return false;
    };
    if host != "xiaoyuzhoufm.com" {
        return false;
    }

    let without_scheme = path
        .strip_prefix("http://")
        .or_else(|| path.strip_prefix("https://"))
        .unwrap_or(path);
    let path_part = without_scheme
        .split_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    path_part.to_ascii_lowercase().starts_with("episode/")
}

pub fn is_m3u(path: &str) -> bool {
    if !is_url(path) {
        return false;
    }
    let base = path.split('?').next().unwrap_or(path);
    let lower = base.to_ascii_lowercase();
    lower.ends_with(".m3u") || lower.ends_with(".m3u8")
}

pub fn is_pls(path: &str) -> bool {
    if !is_url(path) {
        return false;
    }
    let base = path.split('?').next().unwrap_or(path);
    base.to_ascii_lowercase().ends_with(".pls")
}

pub fn is_feed(path: &str) -> bool {
    if !is_url(path) {
        return false;
    }
    let base = path.split('?').next().unwrap_or(path);
    let lower = base.to_ascii_lowercase();
    lower.ends_with(".xml") || lower.ends_with(".rss") || lower.ends_with(".atom")
}

fn track_from_url(url: String) -> Track {
    let without_query = url.split('?').next().unwrap_or(url.as_str());
    let without_scheme = without_query
        .strip_prefix("http://")
        .or_else(|| without_query.strip_prefix("https://"))
        .unwrap_or(without_query);

    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or("stream")
        .to_string();

    let tail = without_scheme.rsplit('/').next().unwrap_or(host.as_str());
    let decoded_tail = decode(tail)
        .map(|v| v.into_owned())
        .unwrap_or_else(|_| tail.to_string());
    let stem = Path::new(tail)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(tail)
        .trim();
    let decoded_stem = Path::new(decoded_tail.as_str())
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(decoded_tail.as_str())
        .trim();
    let title = if stem.is_empty() || stem == "stream" || stem == "rest" {
        host.clone()
    } else {
        decoded_stem.replace('-', " ")
    };

    Track {
        path: url,
        title,
        artist: String::new(),
        stream: true,
        ytdlp: false,
        realtime: false,
        duration_secs: 0,
    }
}

fn normalized_url_host(path: &str) -> Option<String> {
    let without_scheme = path
        .strip_prefix("http://")
        .or_else(|| path.strip_prefix("https://"))
        .unwrap_or(path);
    let host_port = without_scheme.split('/').next().unwrap_or_default();
    if host_port.is_empty() {
        return None;
    }

    Some(
        host_port
            .split(':')
            .next()
            .unwrap_or_default()
            .trim()
            .trim_start_matches("www.")
            .trim_start_matches("m.")
            .to_ascii_lowercase(),
    )
}

#[derive(Debug, Default)]
pub struct Playlist {
    tracks: Vec<Track>,
    order: Vec<usize>,
    pos: usize,
    shuffle: bool,
    repeat: RepeatMode,
    queue: Vec<usize>,
    queued_idx: Option<usize>,
}

impl Playlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, tracks: impl IntoIterator<Item = Track>) {
        for track in tracks {
            let idx = self.tracks.len();
            self.tracks.push(track);
            self.order.push(idx);
        }
    }

    pub fn replace(&mut self, tracks: impl IntoIterator<Item = Track>) {
        self.tracks.clear();
        self.order.clear();
        self.pos = 0;
        self.queue.clear();
        self.queued_idx = None;
        self.add(tracks);
        if self.shuffle && self.tracks.len() > 1 {
            self.do_shuffle();
        }
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn current(&self) -> Option<(Track, usize)> {
        if self.tracks.is_empty() || self.order.is_empty() {
            return None;
        }
        if let Some(idx) = self.queued_idx {
            return Some((self.tracks[idx].clone(), idx));
        }
        let idx = self.order[self.pos];
        Some((self.tracks[idx].clone(), idx))
    }

    pub fn index(&self) -> Option<usize> {
        if self.order.is_empty() {
            None
        } else if let Some(idx) = self.queued_idx {
            Some(idx)
        } else {
            Some(self.order[self.pos])
        }
    }

    pub fn next(&mut self) -> Option<Track> {
        if self.tracks.is_empty() {
            return None;
        }

        if !self.queue.is_empty() {
            let idx = self.queue.remove(0);
            self.queued_idx = Some(idx);
            return Some(self.tracks[idx].clone());
        }

        self.queued_idx = None;

        if self.repeat == RepeatMode::One {
            return Some(self.tracks[self.order[self.pos]].clone());
        }

        if self.pos + 1 < self.order.len() {
            self.pos += 1;
            return Some(self.tracks[self.order[self.pos]].clone());
        }

        if self.repeat == RepeatMode::All {
            self.pos = 0;
            if self.shuffle {
                self.do_shuffle();
            }
            return Some(self.tracks[self.order[self.pos]].clone());
        }

        None
    }

    pub fn prev(&mut self) -> Option<Track> {
        self.queued_idx = None;

        if self.tracks.is_empty() {
            return None;
        }

        if self.pos > 0 {
            self.pos -= 1;
            return Some(self.tracks[self.order[self.pos]].clone());
        }

        if self.repeat == RepeatMode::All {
            self.pos = self.order.len().saturating_sub(1);
            return Some(self.tracks[self.order[self.pos]].clone());
        }

        Some(self.tracks[self.order[self.pos]].clone())
    }

    pub fn set_index(&mut self, idx: usize) {
        self.queued_idx = None;
        if let Some((pos, _)) = self.order.iter().enumerate().find(|(_, i)| **i == idx) {
            self.pos = pos;
        }
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.tracks.is_empty() {
            return;
        }

        if self.shuffle {
            self.do_shuffle();
            return;
        }

        let current = self.order[self.pos];
        self.order = (0..self.tracks.len()).collect();
        self.pos = current;
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.cycle();
    }

    pub fn shuffled(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn queue(&mut self, track_idx: usize) {
        if track_idx < self.tracks.len() && !self.queue.contains(&track_idx) {
            self.queue.push(track_idx);
        }
    }

    pub fn dequeue(&mut self, track_idx: usize) -> bool {
        if let Some(i) = self.queue.iter().position(|idx| *idx == track_idx) {
            self.queue.remove(i);
            return true;
        }
        false
    }

    pub fn queue_position(&self, track_idx: usize) -> Option<usize> {
        self.queue
            .iter()
            .position(|idx| *idx == track_idx)
            .map(|p| p + 1)
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn remove_at(&mut self, idx: usize) -> bool {
        if idx >= self.tracks.len() {
            return false;
        }

        let current_before = if self.order.is_empty() {
            None
        } else {
            Some(self.order[self.pos])
        };

        self.tracks.remove(idx);
        self.order.retain(|track_idx| *track_idx != idx);
        for track_idx in &mut self.order {
            if *track_idx > idx {
                *track_idx -= 1;
            }
        }

        if let Some(queued) = self.queued_idx {
            self.queued_idx = if queued == idx {
                None
            } else if queued > idx {
                Some(queued - 1)
            } else {
                Some(queued)
            };
        }

        let mut new_queue = Vec::with_capacity(self.queue.len());
        for queued in self.queue.iter().copied() {
            if queued == idx {
                continue;
            }
            if queued > idx {
                new_queue.push(queued - 1);
            } else {
                new_queue.push(queued);
            }
        }
        self.queue = new_queue;

        if self.order.is_empty() {
            self.pos = 0;
            self.queued_idx = None;
            self.queue.clear();
            return true;
        }

        if let Some(cur) = current_before {
            if cur == idx {
                if self.pos >= self.order.len() {
                    self.pos = self.order.len() - 1;
                }
            } else {
                let target = if cur > idx { cur - 1 } else { cur };
                if let Some(new_pos) = self.order.iter().position(|track_idx| *track_idx == target)
                {
                    self.pos = new_pos;
                } else if self.pos >= self.order.len() {
                    self.pos = self.order.len() - 1;
                }
            }
        } else if self.pos >= self.order.len() {
            self.pos = self.order.len() - 1;
        }

        true
    }

    pub fn remove_queue_at(&mut self, queue_pos: usize) -> bool {
        if queue_pos >= self.queue.len() {
            return false;
        }
        self.queue.remove(queue_pos);
        true
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }

    pub fn queued_tracks(&self) -> Vec<Track> {
        self.queue
            .iter()
            .filter_map(|track_idx| self.tracks.get(*track_idx).cloned())
            .collect()
    }

    pub fn peek_next(&self) -> Option<Track> {
        if self.tracks.is_empty() || self.order.is_empty() {
            return None;
        }

        if let Some(next_queued) = self.queue.first().copied() {
            return self.tracks.get(next_queued).cloned();
        }

        if self.repeat == RepeatMode::One {
            if let Some(idx) = self.queued_idx {
                return self.tracks.get(idx).cloned();
            }
            return self.tracks.get(self.order[self.pos]).cloned();
        }

        if self.pos + 1 < self.order.len() {
            return self.tracks.get(self.order[self.pos + 1]).cloned();
        }

        if self.repeat == RepeatMode::All && !self.shuffle {
            return self
                .order
                .first()
                .and_then(|idx| self.tracks.get(*idx).cloned());
        }

        None
    }

    fn do_shuffle(&mut self) {
        if self.tracks.len() <= 1 {
            self.order = (0..self.tracks.len()).collect();
            self.pos = 0;
            return;
        }

        let current = self.order[self.pos];
        let mut others: Vec<usize> = (0..self.tracks.len()).filter(|i| *i != current).collect();
        others.shuffle(&mut thread_rng());

        self.order.clear();
        self.order.push(current);
        self.order.extend(others);
        self.pos = 0;
    }
}
