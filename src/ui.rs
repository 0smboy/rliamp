use crate::background::ParticleBackground;
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

const PANEL_WIDTH: usize = 92;

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
const ANSI_MAGENTA: &str = "\x1b[95m";
const ANSI_RED: &str = "\x1b[91m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusArea {
    Provider,
    Playlist,
    Eq,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiLang {
    En,
    Zh,
}

impl UiLang {
    fn detect() -> Self {
        for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
            if let Ok(value) = env::var(key) {
                let v = value.to_lowercase();
                if v.starts_with("zh") || v.contains("zh_") {
                    return UiLang::Zh;
                }
            }
        }
        UiLang::En
    }

    fn toggle(self) -> Self {
        match self {
            UiLang::En => UiLang::Zh,
            UiLang::Zh => UiLang::En,
        }
    }
}

struct EqPreset {
    id: &'static str,
    name_en: &'static str,
    name_zh: &'static str,
    hotkey: Option<char>,
    bands: [f32; 10],
}

const EQ_PRESETS: [EqPreset; 16] = [
    EqPreset {
        id: "flat",
        name_en: "Flat",
        name_zh: "平直",
        hotkey: None,
        bands: [0.0; 10],
    },
    EqPreset {
        id: "rock",
        name_en: "Rock",
        name_zh: "摇滚",
        hotkey: None,
        bands: [5.0, 4.0, 2.0, -1.0, -2.0, 2.0, 4.0, 5.0, 5.0, 5.0],
    },
    EqPreset {
        id: "pop",
        name_en: "Pop",
        name_zh: "流行",
        hotkey: None,
        bands: [-1.0, 2.0, 4.0, 5.0, 4.0, 1.0, -1.0, -1.0, 1.0, 2.0],
    },
    EqPreset {
        id: "jazz",
        name_en: "Jazz",
        name_zh: "爵士",
        hotkey: None,
        bands: [3.0, 4.0, 2.0, 1.0, -1.0, -1.0, 1.0, 2.0, 3.0, 4.0],
    },
    EqPreset {
        id: "classical",
        name_en: "Classical",
        name_zh: "古典",
        hotkey: None,
        bands: [3.0, 2.0, 1.0, 0.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0],
    },
    EqPreset {
        id: "bass-boost",
        name_en: "Bass Boost",
        name_zh: "低频增强",
        hotkey: None,
        bands: [8.0, 6.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    },
    EqPreset {
        id: "treble-boost",
        name_en: "Treble Boost",
        name_zh: "高频增强",
        hotkey: None,
        bands: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0, 5.0, 6.0, 7.0],
    },
    EqPreset {
        id: "vocal",
        name_en: "Vocal",
        name_zh: "人声",
        hotkey: None,
        bands: [-2.0, -1.0, 1.0, 4.0, 5.0, 4.0, 2.0, 0.0, -1.0, -2.0],
    },
    EqPreset {
        id: "electronic",
        name_en: "Electronic",
        name_zh: "电子",
        hotkey: None,
        bands: [6.0, 4.0, 1.0, -1.0, -2.0, 1.0, 3.0, 4.0, 5.0, 6.0],
    },
    EqPreset {
        id: "acoustic",
        name_en: "Acoustic",
        name_zh: "原声",
        hotkey: None,
        bands: [3.0, 3.0, 2.0, 0.0, 1.0, 2.0, 3.0, 3.0, 2.0, 1.0],
    },
    EqPreset {
        id: "mode-1-architect",
        name_en: "1 Architect",
        name_zh: "1 深度思考",
        hotkey: Some('1'),
        bands: [-3.0, -2.0, -1.0, 0.0, 2.0, 3.0, 1.0, 1.0, 2.0, 1.0],
    },
    EqPreset {
        id: "mode-2-spatial",
        name_en: "2 Spatial HiFi",
        name_zh: "2 宇宙空间",
        hotkey: Some('2'),
        bands: [2.0, 1.0, -1.0, -2.0, 0.0, 1.0, 2.0, 3.0, 4.0, 3.0],
    },
    EqPreset {
        id: "mode-3-gym-drive",
        name_en: "3 Gym / Drive",
        name_zh: "3 多巴胺冲击",
        hotkey: Some('3'),
        bands: [4.0, 3.0, 0.0, 0.0, 1.0, 4.0, 3.0, 1.0, 1.0, 0.0],
    },
    EqPreset {
        id: "mode-4-live-reality",
        name_en: "4 Live Reality",
        name_zh: "4 真实现场",
        hotkey: Some('4'),
        bands: [1.0, 1.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0, 3.0, 2.0],
    },
    EqPreset {
        id: "mode-5-theta",
        name_en: "5 Theta Sleep",
        name_zh: "5 睡眠冥想",
        hotkey: Some('5'),
        bands: [1.0, 0.0, -2.0, -2.0, -1.0, -3.0, -3.0, -1.0, 1.0, 2.0],
    },
    EqPreset {
        id: "mode-6-engineer",
        name_en: "6 Engineer",
        name_zh: "6 工程师模式",
        hotkey: Some('6'),
        bands: [-3.0, -2.0, -1.0, 0.0, 2.0, 2.0, 1.0, 1.0, 2.0, 1.0],
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
    lang: UiLang,
    error: Option<String>,
    quitting: bool,
    show_keymap: bool,
    searching: bool,
    search_query: String,
    search_results: Vec<usize>,
    search_cursor: usize,
    prev_focus: FocusArea,
    bg: ParticleBackground,
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
            lang: UiLang::detect(),
            error: None,
            quitting: false,
            show_keymap: false,
            searching: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_cursor: 0,
            prev_focus: FocusArea::Playlist,
            bg: ParticleBackground::new(PANEL_WIDTH, 24),
        };

        if app.provider.is_some() {
            app.reload_provider_playlists();
        }

        app
    }

    pub fn set_eq_preset_by_name(&mut self, name: &str) -> bool {
        let short_key = if name.chars().count() == 1 {
            name.chars().next()
        } else {
            None
        };
        for (idx, preset) in EQ_PRESETS.iter().enumerate() {
            if preset.name_en.eq_ignore_ascii_case(name)
                || preset.id.eq_ignore_ascii_case(name)
                || preset.name_zh == name
                || short_key.is_some() && preset.hotkey == short_key
            {
                self.eq_preset_idx = Some(idx);
                self.apply_eq_preset();
                return true;
            }
        }
        false
    }

    fn tr<'a>(&self, en: &'a str, zh: &'a str) -> &'a str {
        if self.lang == UiLang::Zh {
            zh
        } else {
            en
        }
    }

    fn toggle_language(&mut self) {
        self.lang = self.lang.toggle();
    }

    fn apply_eq_preset_hotkey(&mut self, hotkey: char) -> bool {
        for (idx, preset) in EQ_PRESETS.iter().enumerate() {
            if preset.hotkey == Some(hotkey) {
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

        if matches!(key.code, KeyCode::Char('i') | KeyCode::Char('I')) {
            self.toggle_language();
            return;
        }
        if matches!(
            key.code,
            KeyCode::Char('1')
                | KeyCode::Char('2')
                | KeyCode::Char('3')
                | KeyCode::Char('4')
                | KeyCode::Char('5')
                | KeyCode::Char('6')
        ) {
            if let KeyCode::Char(ch) = key.code {
                let _ = self.apply_eq_preset_hotkey(ch);
            }
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
        self.bg.tick();
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
            Some(idx) => {
                if self.lang == UiLang::Zh {
                    EQ_PRESETS[idx].name_zh
                } else {
                    EQ_PRESETS[idx].name_en
                }
            }
            None => self.tr("Custom", "自定义"),
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
        ];
        lines.extend(self.render_spectrum());
        lines.extend([
            self.render_seek_bar(),
            String::new(),
            self.render_volume(),
            self.render_eq(),
            String::new(),
            self.render_playlist_header(),
        ]);

        lines.extend(self.render_playlist());
        lines.push(String::new());
        lines.extend(self.render_help_lines());

        if let Some(err) = &self.error {
            lines.push(format!("{}: {err}", self.tr("ERR", "错误")));
        }

        self.bg.resize(PANEL_WIDTH, lines.len());
        self.apply_background(&mut lines);
        wrap_frame(lines)
    }

    fn apply_background(&mut self, lines: &mut [String]) {
        for (y, line) in lines.iter_mut().enumerate() {
            let mut chars: Vec<char> = line.chars().collect();
            if chars.is_empty() {
                continue;
            }

            let non_space = chars.iter().rposition(|ch| !ch.is_whitespace());
            let start = match non_space {
                Some(idx) => idx.saturating_add(8),
                None => 0,
            };

            for x in start..chars.len().min(PANEL_WIDTH) {
                if chars[x] != ' ' {
                    continue;
                }
                let bg_ch = self.bg.ch_at(x, y);
                if bg_ch != ' ' {
                    chars[x] = bg_ch;
                }
            }

            *line = chars.into_iter().collect();
        }
    }

    fn render_keymap(&self) -> Vec<String> {
        let mut lines = vec![
            self.tr("K E Y M A P", "按 键 说 明").to_string(),
            String::new(),
            format!("  Space      {}", self.tr("Play / Pause", "播放 / 暂停")),
            format!("  s          {}", self.tr("Stop", "停止")),
            format!("  > .        {}", self.tr("Next track", "下一曲")),
            format!("  < ,        {}", self.tr("Previous track", "上一曲")),
            format!("  ← →        {}", self.tr("Seek +/-5s", "快进/快退 5 秒")),
            format!("  + -        {}", self.tr("Volume up/down", "音量增减")),
            format!("  m          {}", self.tr("Toggle mono", "切换单声道")),
            format!(
                "  e          {}",
                self.tr("Cycle EQ presets", "循环切换 EQ 预设")
            ),
            format!(
                "  1-6        {}",
                self.tr("Quick EQ mode 1-6", "快速 EQ 模式 1-6")
            ),
            format!("  i          {}", self.tr("Toggle EN/ZH", "切换中英文界面")),
            format!(
                "  ↑ ↓        {}",
                self.tr("Playlist scroll / EQ adjust", "播放列表滚动 / EQ 调节")
            ),
            format!(
                "  h l        {}",
                self.tr("EQ cursor left/right", "EQ 光标左/右")
            ),
            format!(
                "  Enter      {}",
                self.tr("Play selected track", "播放选中曲目")
            ),
            format!(
                "  a          {}",
                self.tr("Toggle queue (play next)", "加入/移出队列（下一首）")
            ),
            format!(
                "  /          {}",
                self.tr("Search playlist", "搜索播放列表")
            ),
            format!("  Tab        {}", self.tr("Toggle focus", "切换焦点")),
            format!(
                "  Esc / b    {}",
                self.tr("Back to provider", "返回服务端播放列表")
            ),
            format!("  Ctrl+K     {}", self.tr("This keymap", "显示此按键说明")),
            format!("  q          {}", self.tr("Quit", "退出")),
            String::new(),
            self.tr("Press any key to close", "按任意键关闭")
                .to_string(),
        ];

        if self.provider.is_none() {
            lines.retain(|line| !line.contains("provider") && !line.contains("服务端"));
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
            .unwrap_or_else(|| self.tr("No track loaded", "未加载曲目").to_string());

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
            self.tr("⏸ Paused", "⏸ 暂停")
        } else if self.player.is_playing() && self.current_track_is_stream() {
            self.tr("● Streaming", "● 流媒体")
        } else if self.player.is_playing() {
            self.tr("▶ Playing", "▶ 播放中")
        } else {
            self.tr("■ Stopped", "■ 已停止")
        };

        let gap = PANEL_WIDTH
            .saturating_sub(display_width(&left))
            .saturating_sub(display_width(status))
            .max(1);

        format!("{left}{}{}", " ".repeat(gap), status)
    }

    fn render_spectrum(&mut self) -> Vec<String> {
        let bands = self.vis.analyze(&self.player.samples(2048));
        self.vis.render_neon(bands, self.title_off as u64)
    }

    fn render_seek_bar(&self) -> String {
        if self.current_track_is_stream() && self.player.is_playing() {
            let label = self.tr(" STREAMING ", " 流媒体 ");
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
            "{} {}{} {:+.1}{}",
            self.tr("VOL", "音量"),
            "█".repeat(filled),
            "░".repeat(bar_w.saturating_sub(filled)),
            vol,
            self.tr("dB", "分贝")
        );
        if self.player.mono() {
            line.push_str(self.tr(" [Mono]", " [单声道]"));
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

        format!(
            "{}  {} [{}]",
            self.tr("EQ", "均衡"),
            labels.join(" "),
            self.eq_preset_name()
        )
    }

    fn render_playlist_header(&self) -> String {
        if self.focus == FocusArea::Provider {
            let provider_name = self
                .provider
                .as_ref()
                .map(|p| p.name())
                .unwrap_or("Provider");
            return if self.lang == UiLang::Zh {
                format!("── {provider_name} 播放列表 ──")
            } else {
                format!("── {provider_name} Playlists ──")
            };
        }

        let shuffle = if self.playlist.shuffled() {
            self.tr("[Shuffle*]", "[随机*]")
        } else {
            self.tr("[Shuffle]", "[随机]")
        };

        let repeat = match self.playlist.repeat() {
            RepeatMode::Off => self.tr("[Repeat: Off]", "[循环: 关]").to_string(),
            RepeatMode::All => self.tr("[Repeat: All]", "[循环: 全部]").to_string(),
            RepeatMode::One => self.tr("[Repeat: One]", "[循环: 单曲]").to_string(),
        };

        let queue = if self.playlist.queue_len() > 0 {
            if self.lang == UiLang::Zh {
                format!(" [队列: {}]", self.playlist.queue_len())
            } else {
                format!(" [Queue: {}]", self.playlist.queue_len())
            }
        } else {
            String::new()
        };

        format!(
            "── {} ── {shuffle} {repeat}{queue} ──",
            self.tr("Playlist", "播放列表")
        )
    }

    fn render_playlist(&self) -> Vec<String> {
        if self.focus == FocusArea::Provider {
            if self.prov_loading {
                let provider_name = self
                    .provider
                    .as_ref()
                    .map(|p| p.name())
                    .unwrap_or("provider");
                return vec![if self.lang == UiLang::Zh {
                    format!("  正在加载 {provider_name}...")
                } else {
                    format!("  Loading {provider_name}...")
                }];
            }

            if self.provider_lists.is_empty() {
                return vec![self
                    .tr("  No playlists found.", "  未找到播放列表。")
                    .to_string()];
            }

            let visible = self.pl_visible.min(self.provider_lists.len());
            let scroll = self.prov_cursor.saturating_sub(visible.saturating_sub(1));
            let mut out = Vec::new();
            for idx in scroll..(scroll + visible).min(self.provider_lists.len()) {
                let pl = &self.provider_lists[idx];
                let prefix = if idx == self.prov_cursor { "> " } else { "  " };
                let mut name = if self.lang == UiLang::Zh {
                    format!("{prefix}{} ({} 首)", pl.name, pl.track_count)
                } else {
                    format!("{prefix}{} ({} tracks)", pl.name, pl.track_count)
                };
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
            return vec![self
                .tr("  No tracks loaded", "  没有可播放曲目")
                .to_string()];
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
            return vec![self
                .tr("  Type to search…", "  输入关键字开始搜索…")
                .to_string()];
        }
        if self.search_results.is_empty() {
            return vec![self.tr("  No matches", "  无匹配结果").to_string()];
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

    fn render_help_lines(&self) -> Vec<String> {
        if self.searching {
            if self.lang == UiLang::Zh {
                return vec![format!(
                    "/ {}  (找到 {} 条)  [↑↓]移动 [Enter]播放 [Esc]取消",
                    self.search_query,
                    self.search_results.len()
                )];
            }
            return vec![format!(
                "/ {}  ({} found)  [↑↓]Navigate [Enter]Play [Esc]Cancel",
                self.search_query,
                self.search_results.len()
            )];
        }

        if self.focus == FocusArea::Provider {
            return vec![self
                .tr(
                    "[↑↓]Navigate [Enter]Load [i]Lang [Tab]Focus [Q]Quit",
                    "[↑↓]移动 [Enter]加载 [i]语言 [Tab]焦点 [Q]退出",
                )
                .to_string()];
        }

        let mut line1 = String::from(self.tr("[Spc]⏯ [<>]Trk ", "[空格]⏯ [<>]曲目 "));
        if !self.current_track_is_stream() {
            line1.push_str(self.tr("[←→]Seek ", "[←→]快进/退 "));
        }
        line1.push_str(self.tr(
            "[+-]Vol [m]Mono [e]EQ [1-6]Mode [i]Lang",
            "[+-]音量 [m]单声道 [e]EQ [1-6]模式 [i]语言",
        ));

        let mut line2 = String::new();
        if self.provider.is_some() {
            line2.push_str(self.tr("[Esc]Back ", "[Esc]返回 "));
        }
        line2.push_str(self.tr(
            "[a]Queue [/]Search [Tab]Focus [Q]Quit",
            "[a]队列 [/]搜索 [Tab]焦点 [Q]退出",
        ));

        vec![line1, line2]
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

        if trimmed.starts_with("C L I A M P")
            || trimmed.starts_with("K E Y M A P")
            || trimmed.starts_with("按 键 说 明")
        {
            return paint(ANSI_TITLE, content);
        }

        if trimmed.starts_with("♫ ") {
            return paint(ANSI_YELLOW, content);
        }

        if trimmed.starts_with("VOL ") || trimmed.starts_with("音量 ") {
            let mut vol = colorize_volume_line(content);
            vol = vol.replace("[Mono]", &paint(ANSI_YELLOW_BOLD, "[Mono]"));
            vol = vol.replace("[单声道]", &paint(ANSI_YELLOW_BOLD, "[单声道]"));
            return vol;
        }

        if trimmed.starts_with("EQ  ") || trimmed.starts_with("均衡  ") {
            return colorize_tokens(
                content,
                ANSI_DIM,
                &[("EQ", ANSI_TEXT_BOLD), ("均衡", ANSI_TEXT_BOLD)],
            );
        }

        if trimmed.starts_with("── Playlist ──") || trimmed.starts_with("── 播放列表 ──")
        {
            return colorize_tokens(
                content,
                ANSI_DIM,
                &[
                    ("[Shuffle*]", ANSI_YELLOW),
                    ("[随机*]", ANSI_YELLOW),
                    ("[Repeat: All]", ANSI_YELLOW),
                    ("[循环: 全部]", ANSI_YELLOW),
                    ("[Repeat: One]", ANSI_YELLOW),
                    ("[循环: 单曲]", ANSI_YELLOW),
                    ("[Queue:", ANSI_YELLOW),
                    ("[队列:", ANSI_YELLOW),
                ],
            );
        }

        if trimmed.starts_with("── ")
            && (trimmed.contains(" Playlists ──") || trimmed.contains(" 播放列表 ──"))
        {
            return paint(ANSI_DIM, content);
        }

        if trimmed.starts_with("ERR:") || trimmed.starts_with("错误:") {
            return paint(ANSI_RED, content);
        }

        if trimmed.starts_with("[Spc")
            || trimmed.starts_with("[空格]")
            || trimmed.starts_with("[a]")
            || trimmed.starts_with("[↑↓]")
            || trimmed.starts_with('/')
            || trimmed.starts_with("Press ")
            || trimmed.starts_with("按任意键")
        {
            return paint(ANSI_DIM, content);
        }

        if is_particle_line(trimmed) {
            return colorize_particle_line(content);
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
                ("● 流媒体", ANSI_GREEN_BOLD),
                ("▶ Playing", ANSI_GREEN_BOLD),
                ("▶ 播放中", ANSI_GREEN_BOLD),
                ("⏸ Paused", ANSI_YELLOW_BOLD),
                ("⏸ 暂停", ANSI_YELLOW_BOLD),
                ("■ Stopped", ANSI_DIM),
                ("■ 已停止", ANSI_DIM),
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
            '█' | '▉' | '▊' | '▇' | '▆' => {
                out.push_str(&paint(ANSI_RED, &ch.to_string()))
            }
            '▓' | '▅' | '▄' => out.push_str(&paint(ANSI_YELLOW, &ch.to_string())),
            '▒' | '░' | '▃' | '▂' | '▁' => {
                out.push_str(&paint(ANSI_GREEN, &ch.to_string()))
            }
            '✦' | '•' | '·' | '.' => out.push_str(&paint(ANSI_MAGENTA, &ch.to_string())),
            ' ' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

fn colorize_particle_line(content: &str) -> String {
    let mut out = String::new();
    for ch in content.chars() {
        match ch {
            '█' => out.push_str(&paint(ANSI_GREEN_BOLD, &ch.to_string())),
            'A'..='Z' | '0'..='9' | '@' | '#' | '$' | '%' | '&' | '*' => {
                out.push_str(&paint(ANSI_VOLUME, &ch.to_string()))
            }
            'a'..='z' => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
            '✦' => out.push_str(&paint(ANSI_YELLOW_BOLD, &ch.to_string())),
            '•' => out.push_str(&paint(ANSI_MAGENTA, &ch.to_string())),
            '·' | '.' => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
            ' ' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

fn colorize_volume_line(content: &str) -> String {
    if let Some(rest) = content
        .strip_prefix("VOL ")
        .or_else(|| content.strip_prefix("音量 "))
    {
        let label = if content.starts_with("音量 ") {
            "音量"
        } else {
            "VOL"
        };
        let mut mono_label: Option<&str> = None;
        let body = if let Some(stripped) = rest.strip_suffix(" [Mono]") {
            mono_label = Some("[Mono]");
            stripped
        } else if let Some(stripped) = rest.strip_suffix(" [单声道]") {
            mono_label = Some("[单声道]");
            stripped
        } else {
            rest
        };

        let mut out = String::new();
        out.push_str(&paint(ANSI_TEXT_BOLD, label));
        out.push(' ');

        for ch in body.chars() {
            match ch {
                '█' => out.push_str(&paint(ANSI_VOLUME, &ch.to_string())),
                '░' => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
                ' ' => out.push(' '),
                _ => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
            }
        }
        if let Some(label) = mono_label {
            out.push(' ');
            out.push_str(&paint(ANSI_YELLOW_BOLD, label));
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
    let has_tag = trimmed.contains("STREAMING") || trimmed.contains("流媒体");
    has_tag
        && trimmed.chars().all(|ch| {
            ch == '━' || ch == ' ' || ch.is_ascii_uppercase() || ('一'..='龥').contains(&ch)
        })
}

fn is_spectrum_line(trimmed: &str) -> bool {
    let mut has_bar = false;
    for ch in trimmed.chars() {
        if ch == ' ' {
            continue;
        }
        if matches!(
            ch,
            '▁' | '▂'
                | '▃'
                | '▄'
                | '▅'
                | '▆'
                | '▇'
                | '█'
                | '▉'
                | '▊'
                | '▓'
                | '▒'
                | '░'
                | '✦'
                | '•'
                | '·'
                | '.'
        ) {
            has_bar = true;
            continue;
        }
        return false;
    }
    has_bar
}

fn is_particle_line(trimmed: &str) -> bool {
    let mut has_particle = false;
    for ch in trimmed.chars() {
        if ch == ' ' {
            continue;
        }
        if matches!(
            ch,
            '█'
                | 'A'..='Z'
                | 'a'..='z'
                | '0'..='9'
                | '@'
                | '#'
                | '$'
                | '%'
                | '&'
                | '*'
                | '✦'
                | '•'
                | '·'
                | '.'
        ) {
            has_particle = true;
            continue;
        }
        return false;
    }
    has_particle
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
