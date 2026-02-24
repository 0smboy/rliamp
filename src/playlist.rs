use rand::seq::SliceRandom;
use rand::thread_rng;
use std::path::Path;

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
}

impl Track {
    pub fn from_path(path: impl Into<String>) -> Self {
        let path = path.into();
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
            };
        }

        Track {
            path,
            title: name.to_string(),
            artist: String::new(),
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

#[derive(Debug, Default)]
pub struct Playlist {
    tracks: Vec<Track>,
    order: Vec<usize>,
    pos: usize,
    shuffle: bool,
    repeat: RepeatMode,
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

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn current(&self) -> Option<(Track, usize)> {
        if self.tracks.is_empty() || self.order.is_empty() {
            return None;
        }
        let idx = self.order[self.pos];
        Some((self.tracks[idx].clone(), idx))
    }

    pub fn index(&self) -> Option<usize> {
        if self.order.is_empty() {
            None
        } else {
            Some(self.order[self.pos])
        }
    }

    pub fn next(&mut self) -> Option<Track> {
        if self.tracks.is_empty() {
            return None;
        }

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
