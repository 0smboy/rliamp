use crate::player::Player;
use crate::playlist::{Playlist, RepeatMode};
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

const PANEL_WIDTH: usize = 60;

const ANSI_RESET: &str = "[0m";
const ANSI_BORDER: &str = "[90m";
const ANSI_TEXT: &str = "[37m";
const ANSI_TEXT_BOLD: &str = "[1;37m";
const ANSI_DIM: &str = "[90m";
const ANSI_TITLE: &str = "[1;92m";
const ANSI_GREEN: &str = "[92m";
const ANSI_GREEN_BOLD: &str = "[1;92m";
const ANSI_VOLUME: &str = "[32m";
const ANSI_YELLOW: &str = "[93m";
const ANSI_YELLOW_BOLD: &str = "[1;93m";
const ANSI_RED: &str = "[91m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusArea {
    Playlist,
    Eq,
}

pub struct App {
    player: Player,
    playlist: Playlist,
    vis: Visualizer,
    focus: FocusArea,
    eq_cursor: usize,
    pl_cursor: usize,
    pl_scroll: usize,
    pl_visible: usize,
    title_off: usize,
    error: Option<String>,
    quitting: bool,
}

impl App {
    pub fn new(player: Player, playlist: Playlist) -> Self {
        let sample_rate = player.output_sample_rate();
        Self {
            player,
            playlist,
            vis: Visualizer::new(sample_rate),
            focus: FocusArea::Playlist,
            eq_cursor: 0,
            pl_cursor: 0,
            pl_scroll: 0,
            pl_visible: 5,
            title_off: 0,
            error: None,
            quitting: false,
        }
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
        let frame = self.colorize_frame(&plain).replace('\n', "\r\n");
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

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit(),
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
                } else {
                    self.player.seek(Duration::from_secs(5), true);
                }
            }
            KeyCode::Right => {
                if self.focus == FocusArea::Eq {
                    if self.eq_cursor < 9 {
                        self.eq_cursor += 1;
                    }
                } else {
                    self.player.seek(Duration::from_secs(5), false);
                }
            }
            KeyCode::Up => self.up_action(),
            KeyCode::Down => self.down_action(),
            KeyCode::Char('k') | KeyCode::Char('K') => self.up_action(),
            KeyCode::Char('j') | KeyCode::Char('J') => self.down_action(),
            KeyCode::Enter => {
                self.playlist.set_index(self.pl_cursor);
                self.play_current_track();
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
            _ => {}
        }
    }

    fn up_action(&mut self) {
        if self.focus == FocusArea::Eq {
            let bands = self.player.eq_bands();
            self.player
                .set_eq_band(self.eq_cursor, bands[self.eq_cursor] + 1.0);
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

    fn render(&mut self) -> String {
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

        let bar_w: usize = 22;
        let filled = (frac * bar_w as f32) as usize;
        format!(
            "VOL {}{} {:+.1}dB",
            "█".repeat(filled),
            "░".repeat(bar_w.saturating_sub(filled)),
            vol
        )
    }

    fn render_eq(&self) -> String {
        let bands = self.player.eq_bands();
        let mut labels = vec![
            "70".to_string(),
            "180".to_string(),
            "320".to_string(),
            "600".to_string(),
            "1k".to_string(),
            "3k".to_string(),
            "6k".to_string(),
            "12k".to_string(),
            "14k".to_string(),
            "16k".to_string(),
        ];

        if self.focus == FocusArea::Eq {
            labels[self.eq_cursor] = format!("[{:+.0}]", bands[self.eq_cursor]);
        }

        format!("EQ  {}", labels.join(" "))
    }

    fn render_playlist_header(&self) -> String {
        let shuffle = if self.playlist.shuffled() {
            "[Shuffle*]"
        } else {
            "[Shuffle]"
        };

        let repeat = match self.playlist.repeat() {
            RepeatMode::Off => "[Repeat: Off]".to_string(),
            mode => format!("[Repeat: {mode}]"),
        };

        format!("── Playlist ── {shuffle} {repeat} ──")
    }

    fn render_playlist(&self) -> Vec<String> {
        let tracks = self.playlist.tracks();
        if tracks.is_empty() {
            return vec!["  No tracks loaded".to_string()];
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
            let max_w = PANEL_WIDTH.saturating_sub(6);
            if display_width(&name) > max_w {
                let mut trimmed = truncate_to_width(&name, max_w.saturating_sub(1));
                trimmed.push('.');
                name = trimmed;
            }

            let line = format!("{prefix}{}. {name}", idx + 1);
            if self.focus == FocusArea::Playlist
                && idx == self.pl_cursor
                && !(current_idx == Some(idx) && self.player.is_playing())
            {
                out.push(format!("▶ {}", line.trim_start()));
            } else {
                out.push(line);
            }
        }

        out
    }

    fn render_help(&self) -> String {
        "[Spc]⏯  [<>]Trk [←→]Seek [+-]Vol [Tab]Focus [Q]Quit".to_string()
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

        if trimmed.starts_with("C L I A M P") {
            return paint(ANSI_TITLE, content);
        }

        if trimmed.starts_with("♫ ") {
            return paint(ANSI_YELLOW, content);
        }

        if trimmed.starts_with("VOL ") {
            return colorize_volume_line(content);
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
                ],
            );
        }

        if trimmed.starts_with("ERR:") {
            return paint(ANSI_RED, content);
        }

        if trimmed.starts_with("[Spc") {
            return paint(ANSI_DIM, content);
        }

        if is_spectrum_line(trimmed) {
            return colorize_spectrum_line(content);
        }

        if is_seek_line(trimmed) {
            return colorize_seek_line(content);
        }

        if trimmed.starts_with("▶ ") {
            return paint(ANSI_YELLOW_BOLD, content);
        }

        colorize_tokens(
            content,
            ANSI_TEXT,
            &[
                ("▶ Playing", ANSI_GREEN_BOLD),
                ("⏸ Paused", ANSI_YELLOW_BOLD),
                ("■ Stopped", ANSI_DIM),
            ],
        )
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
    let cursor_idx = chars.iter().position(|ch| *ch == '●' || *ch == 'o');

    let mut out = String::new();
    for (idx, ch) in chars.iter().enumerate() {
        match *ch {
            '●' | 'o' => out.push_str(&paint(ANSI_YELLOW, &ch.to_string())),
            '━' | '=' => {
                if cursor_idx.is_some() && idx <= cursor_idx.unwrap_or(0) {
                    out.push_str(&paint(ANSI_YELLOW, &ch.to_string()));
                } else {
                    out.push_str(&paint(ANSI_DIM, &ch.to_string()));
                }
            }
            '-' => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
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
            '█' | '▇' | '▆' | '@' | '#' => out.push_str(&paint(ANSI_RED, &ch.to_string())),
            '▅' | '▄' | '*' | '+' | '=' => out.push_str(&paint(ANSI_YELLOW, &ch.to_string())),
            '▃' | '▂' | '▁' | '-' | ':' | '.' => {
                out.push_str(&paint(ANSI_GREEN, &ch.to_string()))
            }
            ' ' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out
}

fn colorize_volume_line(content: &str) -> String {
    if let Some(rest) = content.strip_prefix("VOL ") {
        let mut out = String::new();
        out.push_str(&paint(ANSI_TEXT_BOLD, "VOL"));
        out.push(' ');

        for ch in rest.chars() {
            match ch {
                '█' | '#' => out.push_str(&paint(ANSI_VOLUME, &ch.to_string())),
                '░' | '.' => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
                ' ' => out.push(' '),
                _ => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
            }
        }

        return out;
    }

    let mut out = String::new();
    for ch in content.chars() {
        match ch {
            '█' | '#' => out.push_str(&paint(ANSI_VOLUME, &ch.to_string())),
            '░' | '.' => out.push_str(&paint(ANSI_DIM, &ch.to_string())),
            _ => out.push(ch),
        }
    }
    out
}

fn is_seek_line(trimmed: &str) -> bool {
    let mut has_cursor = false;
    for ch in trimmed.chars() {
        if ch == '●' || ch == 'o' {
            has_cursor = true;
            continue;
        }
        if ch == '━' || ch == '=' || ch == '-' {
            continue;
        }
        return false;
    }
    has_cursor
}

fn is_spectrum_line(trimmed: &str) -> bool {
    let mut has_bar = false;
    for ch in trimmed.chars() {
        if ch == ' ' {
            continue;
        }
        if matches!(
            ch,
            '.' | ':'
                | '-'
                | '='
                | '+'
                | '*'
                | '#'
                | '@'
                | '▁'
                | '▂'
                | '▃'
                | '▄'
                | '▅'
                | '▆'
                | '▇'
                | '█'
        ) {
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
