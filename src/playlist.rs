use rand::seq::SliceRandom;
use rand::thread_rng;
use std::path::Path;
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

#[derive(Debug, Clone)]
pub struct Track {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub stream: bool,
    pub ytdlp: bool,
}

impl Track {
    pub fn from_path(path: impl Into<String>) -> Self {
        let path = path.into();
        if is_url(&path) {
            return track_from_url(path);
        }

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
            };
        }

        Track {
            path,
            title: name.to_string(),
            artist: String::new(),
            stream: false,
            ytdlp: false,
        }
    }

    pub fn display_name(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} - {}", self.artist, self.title)
        }
    }
}

pub fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

pub fn is_ytdl(path: &str) -> bool {
    if !is_url(path) {
        return false;
    }

    let without_scheme = path
        .strip_prefix("http://")
        .or_else(|| path.strip_prefix("https://"))
        .unwrap_or(path);
    let host_port = without_scheme.split('/').next().unwrap_or_default();
    if host_port.is_empty() {
        return false;
    }
    let host = host_port
        .split(':')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches("www.")
        .trim_start_matches("m.")
        .to_ascii_lowercase();

    matches!(
        host.as_str(),
        "soundcloud.com" | "youtube.com" | "youtu.be" | "music.youtube.com" | "bandcamp.com"
    ) || host.ends_with(".bandcamp.com")
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
    }
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

    pub fn set_track(&mut self, idx: usize, track: Track) {
        if idx < self.tracks.len() {
            self.tracks[idx] = track;
        }
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
