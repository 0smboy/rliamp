use crate::player::Player;
use crate::playlist::{Playlist, RepeatMode};
use crate::provider::{PlaylistInfo, Provider};
use crate::visualizer::Visualizer;
use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use std::env;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const PANEL_WIDTH: usize = 74;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BORDER: &str = "\x1b[90m";
const ANSI_TEXT: &str = "\x1b[37m";
const ANSI_TEXT_BOLD: &str = "\x1b[1;37m";
const ANSI_DIM: &str = "\x1b[90m";
const ANSI_TITLE: &str = "\x1b[1;92m";
const ANSI_GREEN: &str = "\x1b[92m";
const ANSI_GREEN_BOLD: &str = "\x1b[1;92m";
const ANSI_VOLUME: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[93m";
const ANSI_YELLOW_BOLD: &str = "\x1b[1;93m";
const ANSI_RED: &str = "\x1b[91m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusArea {
    Provider,
    Playlist,
    Eq,
    Search,
}

struct EqPreset {
    name: &'static str,
    bands: [f32; 10],
}

const EQ_PRESETS: [EqPreset; 10] = [
    EqPreset {
        name: "Flat",
        bands: [0.0; 10],
    },
    EqPreset {
        name: "Rock",
        bands: [5.0, 4.0, 2.0, -1.0, -2.0, 2.0, 4.0, 5.0, 5.0, 5.0],
    },
    EqPreset {
        name: "Pop",
        bands: [-1.0, 2.0, 4.0, 5.0, 4.0, 1.0, -1.0, -1.0, 1.0, 2.0],
    },
    EqPreset {
        name: "Jazz",
        bands: [3.0, 4.0, 2.0, 1.0, -1.0, -1.0, 1.0, 2.0, 3.0, 4.0],
    },
    EqPreset {
        name: "Classical",
        bands: [3.0, 2.0, 1.0, 0.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0],
    },
    EqPreset {
        name: "Bass Boost",
        bands: [8.0, 6.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    },
    EqPreset {
        name: "Treble Boost",
        bands: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0, 5.0, 6.0, 7.0],
    },
    EqPreset {
        name: "Vocal",
        bands: [-2.0, -1.0, 1.0, 4.0, 5.0, 4.0, 2.0, 0.0, -1.0, -2.0],
    },
    EqPreset {
        name: "Electronic",
        bands: [6.0, 4.0, 1.0, -1.0, -2.0, 1.0, 3.0, 4.0, 5.0, 6.0],
    },
    EqPreset {
        name: "Acoustic",
        bands: [3.0, 3.0, 2.0, 0.0, 1.0, 2.0, 3.0, 3.0, 2.0, 1.0],
    },
];

pub struct App {
    player: Player,
    playlist: Playlist,
    provider: Option<Box<dyn Provider>>,
    provider_lists: Vec<PlaylistInfo>,
    prov_cursor: usize,
    prov_loading: bool,
    vis: Visualizer,
    focus: FocusArea,
    eq_cursor: usize,
    eq_preset_idx: Option<usize>,
    pl_cursor: usize,
    pl_scroll: usize,
    pl_visible: usize,
    title_off: usize,
    error: Option<String>,
    quitting: bool,
    show_keymap: bool,
    searching: bool,
    search_query: String,
    search_results: Vec<usize>,
    search_cursor: usize,
    prev_focus: FocusArea,
}

impl App {
    pub fn new(player: Player, playlist: Playlist, provider: Option<Box<dyn Provider>>) -> Self {
        let sample_rate = player.output_sample_rate();
        let has_provider = provider.is_some();

        let mut app = Self {
            player,
            playlist,
            provider,
            provider_lists: Vec::new(),
            prov_cursor: 0,
            prov_loading: false,
            vis: Visualizer::new(sample_rate),
            focus: if has_provider {
                FocusArea::Provider
            } else {
                FocusArea::Playlist
            },
            eq_cursor: 0,
            eq_preset_idx: None,
            pl_cursor: 0,
            pl_scroll: 0,
            pl_visible: 5,
            title_off: 0,
            error: None,
            quitting: false,
            show_keymap: false,
            searching: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_cursor: 0,
            prev_focus: FocusArea::Playlist,
        };

        if app.provider.is_some() {
            app.reload_provider_playlists();
        }

        app
    }

    pub fn set_eq_preset_by_name(&mut self, name: &str) -> bool {
        for (idx, preset) in EQ_PRESETS.iter().enumerate() {
            if preset.name.eq_ignore_ascii_case(name) {
                self.eq_preset_idx = Some(idx);
                self.apply_eq_preset();
                return true;
            }
        }
        false
    }

    fn reload_provider_playlists(&mut self) {
        let Some(provider) = self.provider.as_ref() else {
            return;
        };
        self.prov_loading = true;
        match provider.playlists() {
            Ok(mut lists) => {
                lists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.provider_lists = lists;
                if self.prov_cursor >= self.provider_lists.len() {
                    self.prov_cursor = self.provider_lists.len().saturating_sub(1);
                }
                self.error = None;
            }
            Err(err) => {
                self.provider_lists.clear();
                self.error = Some(err.to_string());
            }
        }
        self.prov_loading = false;
    }

    fn load_provider_tracks(&mut self) {
        let Some(provider) = self.provider.as_ref() else {
            return;
        };
        if self.provider_lists.is_empty() || self.prov_cursor >= self.provider_lists.len() {
            return;
        }

        let selected = self.provider_lists[self.prov_cursor].clone();
        self.prov_loading = true;
        match provider.tracks(&selected.id) {
            Ok(tracks) => {
                let was_empty = self.playlist.len() == 0;
                self.playlist.add(tracks);
                self.focus = FocusArea::Playlist;
                if was_empty && self.playlist.len() > 0 {
                    self.pl_cursor = 0;
                    self.playlist.set_index(0);
                    self.play_current_track();
                }
                self.error = None;
            }
            Err(err) => {
                self.error = Some(err.to_string());
            }
        }
        self.prov_loading = false;
    }

    pub fn run(&mut self) -> Result<()> {
        let mut stdout = io::stdout();
        terminal::enable_raw_mode()?;
        stdout.execute(EnterAlternateScreen)?;
        stdout.execute(cursor::Hide)?;

        let run_res = self.run_loop(&mut stdout);

        let _ = stdout.execute(cursor::Show);
        let _ = stdout.execute(LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();

        run_res
    }

    fn run_loop(&mut self, stdout: &mut io::Stdout) -> Result<()> {
        let tick_rate = Duration::from_millis(50);
        let mut last_tick = Instant::now();

        loop {
            self.draw(stdout)?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }

            if last_tick.elapsed() >= tick_rate {
                self.on_tick();
                last_tick = Instant::now();
            }

            if self.quitting {
                break;
            }
        }

        Ok(())
    }

    fn draw(&mut self, stdout: &mut io::Stdout) -> Result<()> {
        let plain = self.render();
        let colored = self.colorize_frame(&plain);
        let term_size = terminal::size().ok().map(|(w, h)| (w as usize, h as usize));
        let centered = if let Some((w, h)) = term_size {
            center_frame(&colored, w, h, PANEL_WIDTH + 6)
        } else {
            colored
        };
        let frame = centered.replace('\n', "\r\n");

        stdout.execute(cursor::MoveTo(0, 0))?;
        stdout.execute(terminal::Clear(ClearType::All))?;
        stdout.execute(Print(frame))?;
        stdout.flush()?;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit();
            return;
        }

        if self.show_keymap {
            self.show_keymap = false;
            return;
        }

        if self.searching {
            self.handle_search_key(key);
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('k') | KeyCode::Char('K'))
        {
            self.show_keymap = true;
            return;
        }

        if self.focus == FocusArea::Provider {
            self.handle_provider_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit(),
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('b') | KeyCode::Char('B') => {
                if self.provider.is_some() {
                    self.focus = FocusArea::Provider;
                }
            }
            KeyCode::Char(' ') | KeyCode::Char('p') | KeyCode::Char('P') => {
                if !self.player.is_playing() {
                    self.play_current_track();
                } else {
                    self.player.toggle_pause();
                }
            }
            KeyCode::Char('s') | KeyCode::Char('S') => self.player.stop(),
            KeyCode::Char('>') | KeyCode::Char('.') => self.next_track(),
            KeyCode::Char('<') | KeyCode::Char(',') => self.prev_track(),
            KeyCode::Left => {
                if self.focus == FocusArea::Eq {
                    if self.eq_cursor > 0 {
                        self.eq_cursor -= 1;
                    }
                } else if !self.current_track_is_stream() {
                    self.player.seek(Duration::from_secs(5), true);
                }
            }
            KeyCode::Right => {
                if self.focus == FocusArea::Eq {
                    if self.eq_cursor < 9 {
                        self.eq_cursor += 1;
                    }
                } else if !self.current_track_is_stream() {
                    self.player.seek(Duration::from_secs(5), false);
                }
            }
            KeyCode::Up => self.up_action(),
            KeyCode::Down => self.down_action(),
            KeyCode::Char('k') | KeyCode::Char('K') => self.up_action(),
            KeyCode::Char('j') | KeyCode::Char('J') => self.down_action(),
            KeyCode::Enter => {
                if self.focus == FocusArea::Playlist {
                    self.playlist.set_index(self.pl_cursor);
                    self.play_current_track();
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let next = self.player.volume() + 1.0;
                self.player.set_volume(next);
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                let next = self.player.volume() - 1.0;
                self.player.set_volume(next);
            }
            KeyCode::Char('r') | KeyCode::Char('R') => self.playlist.cycle_repeat(),
            KeyCode::Char('z') | KeyCode::Char('Z') => self.playlist.toggle_shuffle(),
            KeyCode::Tab => {
                self.focus = if self.focus == FocusArea::Playlist {
                    FocusArea::Eq
                } else {
                    FocusArea::Playlist
                };
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                if self.focus == FocusArea::Eq && self.eq_cursor > 0 {
                    self.eq_cursor -= 1;
                }
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                if self.focus == FocusArea::Eq && self.eq_cursor < 9 {
                    self.eq_cursor += 1;
                }
            }
            KeyCode::Char('e') | KeyCode::Char('E') => self.cycle_eq_preset(),
            KeyCode::Char('m') | KeyCode::Char('M') => self.player.toggle_mono(),
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if self.focus == FocusArea::Playlist && self.pl_cursor < self.playlist.len() {
                    if !self.playlist.dequeue(self.pl_cursor) {
                        self.playlist.queue(self.pl_cursor);
                    }
                }
            }
            KeyCode::Char('/') => self.start_search(),
            _ => {}
        }
    }

    fn handle_provider_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit(),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                if self.prov_cursor > 0 {
                    self.prov_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                if self.prov_cursor + 1 < self.provider_lists.len() {
                    self.prov_cursor += 1;
                }
            }
            KeyCode::Char(' ') | KeyCode::Char('p') | KeyCode::Char('P') => {
                if !self.player.is_playing() {
                    self.play_current_track();
                } else {
                    self.player.toggle_pause();
                }
            }
            KeyCode::Enter => self.load_provider_tracks(),
            KeyCode::Tab => {
                if self.playlist.len() > 0 {
                    self.focus = FocusArea::Playlist;
                }
            }
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.searching = false;
                self.focus = self.prev_focus;
            }
            KeyCode::Enter => {
                if let Some(idx) = self.search_results.get(self.search_cursor).copied() {
                    self.playlist.set_index(idx);
                    self.pl_cursor = idx;
                    self.adjust_scroll();
                    self.play_current_track();
                }
                self.searching = false;
                self.focus = FocusArea::Playlist;
            }
            KeyCode::Up => {
                if self.search_cursor > 0 {
                    self.search_cursor -= 1;
                }
            }
            KeyCode::Down => {
                if self.search_cursor + 1 < self.search_results.len() {
                    self.search_cursor += 1;
                }
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.update_search();
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.search_query.push(ch);
                    self.update_search();
                }
            }
            _ => {}
        }
    }

    fn up_action(&mut self) {
        if self.focus == FocusArea::Eq {
            let bands = self.player.eq_bands();
            self.player
                .set_eq_band(self.eq_cursor, bands[self.eq_cursor] + 1.0);
            self.eq_preset_idx = None;
        } else if self.pl_cursor > 0 {
            self.pl_cursor -= 1;
            self.adjust_scroll();
        }
    }

    fn down_action(&mut self) {
        if self.focus == FocusArea::Eq {
            let bands = self.player.eq_bands();
            self.player
                .set_eq_band(self.eq_cursor, bands[self.eq_cursor] - 1.0);
            self.eq_preset_idx = None;
        } else if self.pl_cursor + 1 < self.playlist.len() {
            self.pl_cursor += 1;
            self.adjust_scroll();
        }
    }

    fn on_tick(&mut self) {
        if self.player.is_playing() && !self.player.is_paused() && self.player.track_done() {
            self.next_track();
        }
        self.title_off = self.title_off.wrapping_add(1);
    }

    fn quit(&mut self) {
        self.player.close();
        self.quitting = true;
    }

    fn next_track(&mut self) {
        if let Some(track) = self.playlist.next() {
            if let Some(idx) = self.playlist.index() {
                self.pl_cursor = idx;
                self.adjust_scroll();
            }
            self.title_off = 0;
            if let Err(err) = self.player.play(&track.path) {
                self.error = Some(err.to_string());
            } else {
                self.error = None;
            }
        } else {
            self.player.stop();
        }
    }

    fn prev_track(&mut self) {
        if self.player.position() > Duration::from_secs(3) {
            let pos = self.player.position();
            self.player.seek(pos, true);
            return;
        }

        if let Some(track) = self.playlist.prev() {
            if let Some(idx) = self.playlist.index() {
                self.pl_cursor = idx;
                self.adjust_scroll();
            }
            self.title_off = 0;
            if let Err(err) = self.player.play(&track.path) {
                self.error = Some(err.to_string());
            } else {
                self.error = None;
            }
        }
    }

    fn play_current_track(&mut self) {
        if let Some((track, _)) = self.playlist.current() {
            self.title_off = 0;
            if let Err(err) = self.player.play(&track.path) {
                self.error = Some(err.to_string());
            } else {
                self.error = None;
            }
        }
    }

    fn adjust_scroll(&mut self) {
        if self.pl_cursor < self.pl_scroll {
            self.pl_scroll = self.pl_cursor;
        }

        if self.pl_cursor >= self.pl_scroll + self.pl_visible {
            self.pl_scroll = self.pl_cursor - self.pl_visible + 1;
        }
    }

    fn start_search(&mut self) {
        self.searching = true;
        self.search_query.clear();
        self.search_results.clear();
        self.search_cursor = 0;
        self.prev_focus = self.focus;
        self.focus = FocusArea::Search;
    }

    fn update_search(&mut self) {
        self.search_results.clear();
        self.search_cursor = 0;

        if self.search_query.is_empty() {
            return;
        }

        let query = self.search_query.to_lowercase();
        for (idx, track) in self.playlist.tracks().iter().enumerate() {
            if track.display_name().to_lowercase().contains(&query) {
                self.search_results.push(idx);
            }
        }
    }

    fn cycle_eq_preset(&mut self) {
        let next = match self.eq_preset_idx {
            Some(idx) => (idx + 1) % EQ_PRESETS.len(),
            None => 0,
        };
        self.eq_preset_idx = Some(next);
        self.apply_eq_preset();
    }

    fn apply_eq_preset(&mut self) {
        let Some(idx) = self.eq_preset_idx else {
            return;
        };
        let preset = &EQ_PRESETS[idx];
        for (i, gain) in preset.bands.iter().enumerate() {
            self.player.set_eq_band(i, *gain);
        }
    }

    fn eq_preset_name(&self) -> &str {
        match self.eq_preset_idx {
            Some(idx) => EQ_PRESETS[idx].name,
            None => "Custom",
        }
    }

    fn current_track_is_stream(&self) -> bool {
        self.playlist
            .current()
            .map(|(t, _)| t.stream)
            .unwrap_or(false)
    }

    fn render(&mut self) -> String {
        if self.show_keymap {
            return wrap_frame(self.render_keymap());
        }

        let mut lines = vec![
            self.render_title(),
            self.render_track_info(),
            self.render_time_status(),
            String::new(),
            self.render_spectrum(),
            self.render_seek_bar(),
            String::new(),
            self.render_volume(),
            self.render_eq(),
            String::new(),
            self.render_playlist_header(),
        ];

        lines.extend(self.render_playlist());
        lines.push(String::new());
        lines.push(self.render_help());

        if let Some(err) = &self.error {
            lines.push(format!("ERR: {err}"));
        }

        wrap_frame(lines)
    }

    fn render_keymap(&self) -> Vec<String> {
        let mut lines = vec![
            "K E Y M A P".to_string(),
            String::new(),
            "  Space      Play / Pause".to_string(),
            "  s          Stop".to_string(),
            "  > .        Next track".to_string(),
            "  < ,        Previous track".to_string(),
            "  ← →        Seek +/-5s".to_string(),
            "  + -        Volume up/down".to_string(),
            "  m          Toggle mono".to_string(),
            "  e          Cycle EQ preset".to_string(),
            "  ↑ ↓        Playlist scroll / EQ adjust".to_string(),
            "  h l        EQ cursor left/right".to_string(),
            "  Enter      Play selected track".to_string(),
            "  a          Toggle queue (play next)".to_string(),
            "  /          Search playlist".to_string(),
            "  Tab        Toggle focus".to_string(),
            "  Esc / b    Back to provider".to_string(),
            "  Ctrl+K     This keymap".to_string(),
            "  q          Quit".to_string(),
            String::new(),
            "Press any key to close".to_string(),
        ];

        if self.provider.is_none() {
            lines.retain(|line| !line.contains("Back to provider"));
        }

        if lines.iter().any(|line| display_width(line) > PANEL_WIDTH) {
            for line in &mut lines {
                *line = truncate_to_width(line, PANEL_WIDTH);
            }
        }

        lines
    }

    fn render_title(&self) -> String {
        "C L I A M P".to_string()
    }

    fn render_track_info(&self) -> String {
        let name = self
            .playlist
            .current()
            .map(|(track, _)| track.display_name())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "No track loaded".to_string());

        let max_w = PANEL_WIDTH.saturating_sub(4);
        let chars: Vec<char> = name.chars().collect();

        if chars.len() <= max_w {
            return format!("♫ {name}");
        }

        let mut padded = chars.clone();
        padded.extend("   ♫   ".chars());
        let total = padded.len();
        let off = if total == 0 {
            0
        } else {
            self.title_off % total
        };

        let mut display = String::new();
        for i in 0..max_w {
            display.push(padded[(off + i) % total]);
        }

        format!("♫ {display}")
    }

    fn render_time_status(&self) -> String {
        let pos = self.player.position();
        let dur = self.player.duration();

        let pos_min = pos.as_secs() / 60;
        let pos_sec = pos.as_secs() % 60;
        let dur_min = dur.as_secs() / 60;
        let dur_sec = dur.as_secs() % 60;

        let left = format!("{pos_min:02}:{pos_sec:02} / {dur_min:02}:{dur_sec:02}");
        let status = if self.player.is_playing() && self.player.is_paused() {
            "⏸ Paused"
        } else if self.player.is_playing() && self.current_track_is_stream() {
            "● Streaming"
        } else if self.player.is_playing() {
            "▶ Playing"
        } else {
            "■ Stopped"
        };

        let gap = PANEL_WIDTH
            .saturating_sub(display_width(&left))
            .saturating_sub(display_width(status))
            .max(1);

        format!("{left}{}{}", " ".repeat(gap), status)
    }

    fn render_spectrum(&mut self) -> String {
        let bands = self.vis.analyze(&self.player.samples(2048));
        self.vis.render(bands)
    }

    fn render_seek_bar(&self) -> String {
        if self.current_track_is_stream() && self.player.is_playing() {
            let label = " STREAMING ";
            let total = PANEL_WIDTH.saturating_sub(display_width(label));
            let left = total / 2;
            let right = total.saturating_sub(left);
            return format!("{}{}{}", "━".repeat(left), label, "━".repeat(right));
        }

        let pos = self.player.position();
        let dur = self.player.duration();

        let mut progress = 0.0;
        if dur > Duration::ZERO {
            progress = (pos.as_secs_f64() / dur.as_secs_f64()).clamp(0.0, 1.0);
        }

        let filled = (progress * (PANEL_WIDTH.saturating_sub(1)) as f64) as usize;
        format!(
            "{}●{}",
            "━".repeat(filled),
            "━".repeat(PANEL_WIDTH.saturating_sub(filled + 1))
        )
    }

    fn render_volume(&self) -> String {
        let vol = self.player.volume();
        let frac = ((vol + 30.0) / 36.0).clamp(0.0, 1.0);

        let bar_w: usize = 30;
        let filled = (frac * bar_w as f32) as usize;
        let mut line = format!(
            "VOL {}{} {:+.1}dB",
            "█".repeat(filled),
            "░".repeat(bar_w.saturating_sub(filled)),
            vol
        );
        if self.player.mono() {
            line.push_str(" [Mono]");
        }
        line
    }

    fn render_eq(&self) -> String {
        let bands = self.player.eq_bands();
        let base_labels = [
            "70", "180", "320", "600", "1k", "3k", "6k", "12k", "14k", "16k",
        ];
        let mut labels = Vec::with_capacity(base_labels.len());

        for (idx, base) in base_labels.iter().enumerate() {
            let mut label = if bands[idx].abs() >= 0.5 {
                format!("{:+.0}", bands[idx])
            } else {
                (*base).to_string()
            };
            if self.focus == FocusArea::Eq && idx == self.eq_cursor {
                label = format!("[{label}]");
            }
            labels.push(label);
        }

        format!("EQ  {} [{}]", labels.join(" "), self.eq_preset_name())
    }

    fn render_playlist_header(&self) -> String {
        if self.focus == FocusArea::Provider {
            let provider_name = self
                .provider
                .as_ref()
                .map(|p| p.name())
                .unwrap_or("Provider");
            return format!("── {provider_name} Playlists ──");
        }

        let shuffle = if self.playlist.shuffled() {
            "[Shuffle*]"
        } else {
            "[Shuffle]"
        };

        let repeat = match self.playlist.repeat() {
            RepeatMode::Off => "[Repeat: Off]".to_string(),
            mode => format!("[Repeat: {mode}]"),
        };

        let queue = if self.playlist.queue_len() > 0 {
            format!(" [Queue: {}]", self.playlist.queue_len())
        } else {
            String::new()
        };

        format!("── Playlist ── {shuffle} {repeat}{queue} ──")
    }

    fn render_playlist(&self) -> Vec<String> {
        if self.focus == FocusArea::Provider {
            if self.prov_loading {
                let provider_name = self
                    .provider
                    .as_ref()
                    .map(|p| p.name())
                    .unwrap_or("provider");
                return vec![format!("  Loading {provider_name}...")];
            }

            if self.provider_lists.is_empty() {
                return vec!["  No playlists found.".to_string()];
            }

            let visible = self.pl_visible.min(self.provider_lists.len());
            let scroll = self.prov_cursor.saturating_sub(visible.saturating_sub(1));
            let mut out = Vec::new();
            for idx in scroll..(scroll + visible).min(self.provider_lists.len()) {
                let pl = &self.provider_lists[idx];
                let prefix = if idx == self.prov_cursor { "> " } else { "  " };
                let mut name = format!("{prefix}{} ({} tracks)", pl.name, pl.track_count);
                if display_width(&name) > PANEL_WIDTH {
                    let mut trimmed = truncate_to_width(&name, PANEL_WIDTH.saturating_sub(1));
                    trimmed.push('…');
                    name = trimmed;
                }
                out.push(name);
            }
            return out;
        }

        let tracks = self.playlist.tracks();
        if tracks.is_empty() {
            return vec!["  No tracks loaded".to_string()];
        }

        if self.searching {
            return self.render_search_results();
        }

        let current_idx = self.playlist.index();
        let visible = self.pl_visible.min(tracks.len());

        let mut scroll = self.pl_scroll;
        if scroll + visible > tracks.len() {
            scroll = tracks.len().saturating_sub(visible);
        }

        let mut out = Vec::new();
        for idx in scroll..(scroll + visible).min(tracks.len()) {
            let prefix = if current_idx == Some(idx) && self.player.is_playing() {
                "▶ "
            } else {
                "  "
            };

            let mut name = tracks[idx].display_name();
            let queue_suffix = self
                .playlist
                .queue_position(idx)
                .map(|qp| format!(" [Q{qp}]"))
                .unwrap_or_default();

            let max_w = PANEL_WIDTH
                .saturating_sub(6)
                .saturating_sub(display_width(&queue_suffix));
            if display_width(&name) > max_w {
                let mut trimmed = truncate_to_width(&name, max_w.saturating_sub(1));
                trimmed.push('…');
                name = trimmed;
            }

            let line = format!("{prefix}{}. {name}{queue_suffix}", idx + 1);
            if self.focus == FocusArea::Playlist
                && idx == self.pl_cursor
                && !(current_idx == Some(idx) && self.player.is_playing())
            {
                out.push(format!("> {}", line.trim_start()));
            } else {
                out.push(line);
            }
        }

        out
    }

    fn render_search_results(&self) -> Vec<String> {
        if self.search_query.is_empty() {
            return vec!["  Type to search…".to_string()];
        }
        if self.search_results.is_empty() {
            return vec!["  No matches".to_string()];
        }

        let tracks = self.playlist.tracks();
        let current_idx = self.playlist.index();
        let visible = self.pl_visible.min(self.search_results.len());
        let scroll = self.search_cursor.saturating_sub(visible.saturating_sub(1));

        let mut out = Vec::new();
        for j in scroll..(scroll + visible).min(self.search_results.len()) {
            let idx = self.search_results[j];
            let prefix = if current_idx == Some(idx) && self.player.is_playing() {
                "▶ "
            } else {
                "  "
            };

            let mut name = tracks[idx].display_name();
            let max_w = PANEL_WIDTH.saturating_sub(6);
            if display_width(&name) > max_w {
                let mut trimmed = truncate_to_width(&name, max_w.saturating_sub(1));
                trimmed.push('…');
                name = trimmed;
            }

            let line = format!("{prefix}{}. {name}", idx + 1);
            if j == self.search_cursor {
                out.push(format!("> {}", line.trim_start()));
            } else {
                out.push(line);
            }
        }

        out
    }

    fn render_help(&self) -> String {
        if self.searching {
            return format!(
                "/ {}  ({} found)  [↑↓]Navigate [Enter]Play [Esc]Cancel",
                self.search_query,
                self.search_results.len()
            );
        }

        if self.focus == FocusArea::Provider {
            return "[↑↓]Navigate [Enter]Load [Tab]Focus [Q]Quit".to_string();
        }

        let mut help = String::from("[Spc]⏯ [<>]Trk ");
        if !self.current_track_is_stream() {
            help.push_str("[←→]Seek ");
        }
        if self.provider.is_some() {
            help.push_str("[Esc]Back ");
        }
        help.push_str("[+-]Vol [m]Mono [e]EQ [a]Q [/]Src [Tab] [Q]Quit");
        help
    }

    fn colorize_frame(&self, frame: &str) -> String {
        if !colors_enabled() {
            return frame.to_string();
        }

        let mut out = Vec::new();
        for line in frame.lines() {
            out.push(self.colorize_line(line));
        }
        out.join("\n")
    }

    fn colorize_line(&self, line: &str) -> String {
        if (line.starts_with('╭') && line.ends_with('╮'))
            || (line.starts_with('╰') && line.ends_with('╯'))
        {
            return paint(ANSI_BORDER, line);
        }

        let inner_prefix = "│  ";
        let inner_suffix = "  │";
        if line.starts_with(inner_prefix) && line.ends_with(inner_suffix) {
            let content = &line[inner_prefix.len()..line.len() - inner_suffix.len()];
            let styled_content = self.colorize_content(content);
            return format!(
                "{}│{}  {}  {}│{}",
                ANSI_BORDER, ANSI_RESET, styled_content, ANSI_BORDER, ANSI_RESET
            );
        }

        if line.starts_with('│') && line.ends_with('│') {
            return paint(ANSI_BORDER, line);
        }

        line.to_string()
    }

    fn colorize_content(&self, content: &str) -> String {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return content.to_string();
        }

        if trimmed.starts_with("C L I A M P") || trimmed.starts_with("K E Y M A P") {
            return paint(ANSI_TITLE, content);
        }

        if trimmed.starts_with("♫ ") {
            return paint(ANSI_YELLOW, content);
        }

        if trimmed.starts_with("VOL ") {
            let mut vol = colorize_volume_line(content);
            vol = vol.replace("[Mono]", &paint(ANSI_YELLOW_BOLD, "[Mono]"));
            return vol;
        }

        if trimmed.starts_with("EQ  ") {
            return colorize_tokens(content, ANSI_DIM, &[("EQ", ANSI_TEXT_BOLD)]);
        }

        if trimmed.starts_with("── Playlist ──") {
            return colorize_tokens(
                content,
                ANSI_DIM,
                &[
                    ("[Shuffle*]", ANSI_YELLOW),
                    ("[Repeat: All]", ANSI_YELLOW),
                    ("[Repeat: One]", ANSI_YELLOW),
                    ("[Queue:", ANSI_YELLOW),
                ],
            );
        }

        if trimmed.starts_with("── ") && trimmed.contains(" Playlists ──") {
            return paint(ANSI_DIM, content);
        }

        if trimmed.starts_with("ERR:") {
            return paint(ANSI_RED, content);
        }

        if trimmed.starts_with("[Spc")
            || trimmed.starts_with("[↑↓]")
            || trimmed.starts_with('/')
            || trimmed.starts_with("Press ")
        {
            return paint(ANSI_DIM, content);
        }

        if is_streaming_seek_line(trimmed) {
            return paint(ANSI_YELLOW, content);
        }

        if is_spectrum_line(trimmed) {
            return colorize_spectrum_line(content);
        }

        if is_seek_line(trimmed) {
            return colorize_seek_line(content);
        }

        if trimmed.starts_with("▶ ") || trimmed.starts_with("> ") {
            return paint(ANSI_YELLOW_BOLD, content);
        }

        let mut styled = colorize_tokens(
            content,
            ANSI_TEXT,
            &[
                ("● Streaming", ANSI_GREEN_BOLD),
                ("▶ Playing", ANSI_GREEN_BOLD),
                ("⏸ Paused", ANSI_YELLOW_BOLD),
                ("■ Stopped", ANSI_DIM),
                ("[Q", ANSI_YELLOW),
            ],
        );
        styled = styled.replace("[Q", &format!("{ANSI_YELLOW}[Q{ANSI_TEXT}"));
        styled
    }
}

fn wrap_frame(lines: Vec<String>) -> String {
    let inner = PANEL_WIDTH + 4;
    let mut out = String::new();

    out.push('╭');
    out.push_str(&"─".repeat(inner));
    out.push('╮');
    out.push('\n');

    out.push('│');
    out.push_str(&" ".repeat(inner));
    out.push('│');
    out.push('\n');

    for line in lines {
        let clipped = truncate_to_width(&line, PANEL_WIDTH);
        let padded = pad_to_width(&clipped, PANEL_WIDTH);
        out.push_str("│  ");
        out.push_str(&padded);
        out.push_str("  │\n");
    }

    out.push('│');
    out.push_str(&" ".repeat(inner));
    out.push('│');
    out.push('\n');

    out.push('╰');
    out.push_str(&"─".repeat(inner));
    out.push('╯');

    out
}

fn center_frame(frame: &str, term_w: usize, term_h: usize, frame_w: usize) -> String {
    let lines: Vec<&str> = frame.lines().collect();
    let frame_h = lines.len();

    let pad_left = term_w.saturating_sub(frame_w) / 2;
    let pad_top = term_h.saturating_sub(frame_h) / 2;

    let mut out = String::new();
    out.push_str(&"\n".repeat(pad_top));

    for (i, line) in lines.iter().enumerate() {
        out.push_str(&" ".repeat(pad_left));
        out.push_str(line);
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }

    out
}

fn paint(color: &str, s: &str) -> String {
    format!("{color}{s}{ANSI_RESET}")
}

fn colorize_tokens(content: &str, base: &str, tokens: &[(&str, &str)]) -> String {
    let mut out = format!("{base}{content}{ANSI_RESET}");
    for (token, color) in tokens {
        let replacement = format!("{color}{token}{base}");
        out = out.replace(token, &replacement);
    }
    format!("{out}{ANSI_RESET}")
}

fn colorize_seek_line(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let cursor_idx = chars.iter().position(|ch| *ch == '●');

    let mut out = String::new();
    for (idx, ch) in chars.iter().enumerate() {
        match *ch {
            '●' => out.push_str(&paint(ANSI_YELLOW, &ch.to_string())),
            '━' => {
                if cursor_idx.is_some() && idx <= cursor_idx.unwrap_or(0) {
                    out.push_str(&paint(ANSI_YELLOW, &ch.to_string()));
                } else {
                    out.push_str(&paint(ANSI_DIM, &ch.to_string()));
                }
            }
            ' ' => out.push(' '),
            _ => out.push(*ch),
        }
    }
    out
}

fn colorize_spectrum_line(content: &str) -> String {
    let mut out = String::new();
    for ch in content.chars() {
        match ch {
            '█' | '▇' | '▆' => out.push_str(&paint(ANSI_RED, &ch.to_string())),
            '▅' | '▄' => out.push_str(&paint(ANSI_YELLOW, &ch.to_string())),
            '▃' | '▂' | '▁' => out.push_str(&paint(ANSI_GREEN, &ch.to_string())),
            ' ' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

fn colorize_volume_line(content: &str) -> String {
    if let Some(rest) = content.strip_prefix("VOL ") {
        let mut mono = false;
        let body = if let Some(stripped) = rest.strip_suffix(" [Mono]") {
            mono = true;
            stripped
        } else {
            rest
        };

        let mut out = String::new();
        out.push_str(&paint(ANSI_TEXT_BOLD, "VOL"));
        out.push(' ');

        for ch in body.chars() {
            match ch {
                '█' => out.push_str(&paint(ANSI_VOLUME, &ch.to_string())),
                '░' => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
                ' ' => out.push(' '),
                _ => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
            }
        }
        if mono {
            out.push(' ');
            out.push_str(&paint(ANSI_YELLOW_BOLD, "[Mono]"));
        }
        return out;
    }

    content.to_string()
}

fn is_seek_line(trimmed: &str) -> bool {
    let mut has_cursor = false;
    for ch in trimmed.chars() {
        if ch == '●' {
            has_cursor = true;
            continue;
        }
        if ch == '━' {
            continue;
        }
        return false;
    }
    has_cursor
}

fn is_streaming_seek_line(trimmed: &str) -> bool {
    trimmed.contains("STREAMING")
        && trimmed
            .chars()
            .all(|ch| ch == '━' || ch == ' ' || ch.is_ascii_uppercase())
}

fn is_spectrum_line(trimmed: &str) -> bool {
    let mut has_bar = false;
    for ch in trimmed.chars() {
        if ch == ' ' {
            continue;
        }
        if matches!(ch, '▁' | '▂' | '▃' | '▄' | '▅' | '▆' | '▇' | '█') {
            has_bar = true;
            continue;
        }
        return false;
    }
    has_bar
}

fn colors_enabled() -> bool {
    if let Ok(term) = env::var("TERM") {
        if term.trim().eq_ignore_ascii_case("dumb") {
            return false;
        }
    }

    true
}

fn is_cjk_locale() -> bool {
    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(value) = env::var(key) {
            let v = value.to_lowercase();
            if v.starts_with("zh") || v.starts_with("ja") || v.starts_with("ko") {
                return true;
            }
            if v.contains("zh_") || v.contains("ja_") || v.contains("ko_") {
                return true;
            }
        }
    }
    false
}

fn display_width(s: &str) -> usize {
    if is_cjk_locale() {
        UnicodeWidthStr::width_cjk(s)
    } else {
        UnicodeWidthStr::width(s)
    }
}

fn pad_to_width(s: &str, width: usize) -> String {
    let current = display_width(s);
    if current >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - current))
    }
}

fn truncate_to_width(s: &str, max_width: usize) -> String {
    if display_width(s) <= max_width {
        return s.to_string();
    }

    let mut out = String::new();
    let mut used = 0usize;
    let cjk = is_cjk_locale();

    for ch in s.chars() {
        let w = if cjk {
            UnicodeWidthChar::width_cjk(ch).unwrap_or(0)
        } else {
            UnicodeWidthChar::width(ch).unwrap_or(0)
        };

        if used + w > max_width {
            break;
        }

        used += w;
        out.push(ch);
    }

    out
}
