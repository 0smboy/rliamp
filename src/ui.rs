use crate::background::ParticleBackground;
use crate::player::Player;
use crate::playlist::{Playlist, RepeatMode};
use crate::provider::{PlaylistInfo, Provider};
use crate::visualizer::Visualizer;
use crate::ytdlp;
use anyhow::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const PANEL_WIDTH: usize = 92;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BORDER: &str = "\x1b[90m";
const ANSI_TEXT: &str = "\x1b[37m";
const ANSI_TEXT_BOLD: &str = "\x1b[1;37m";
const ANSI_DIM: &str = "\x1b[90m";
const ANSI_GREEN_BOLD: &str = "\x1b[1;92m";
const ANSI_VOLUME: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[93m";
const ANSI_YELLOW_BOLD: &str = "\x1b[1;93m";
const ANSI_MAGENTA: &str = "\x1b[95m";
const ANSI_RED: &str = "\x1b[91m";

const DEFAULT_VIS_ROWS: usize = 4;
const EXPANDED_VIS_ROWS: usize = 20;
const TICK_MS_ACTIVE: u64 = 50;
const TICK_MS_VIS_OFF: u64 = 200;

struct ThemeEntry {
    name: &'static str,
    accent: &'static str,
    muted: &'static str,
    value: &'static str,
    title: &'static str,
    spectrum_hi: &'static str,
    spectrum_mid: &'static str,
    spectrum_low: &'static str,
    spectrum_spark: &'static str,
}

const THEMES: [ThemeEntry; 20] = [
    ThemeEntry {
        name: "Neo Mint",
        accent: "\x1b[38;5;120m",
        muted: "\x1b[38;5;147m",
        value: "\x1b[38;5;157m",
        title: "\x1b[1;38;5;120m",
        spectrum_hi: "\x1b[38;5;122m",
        spectrum_mid: "\x1b[38;5;120m",
        spectrum_low: "\x1b[38;5;84m",
        spectrum_spark: "\x1b[38;5;159m",
    },
    ThemeEntry {
        name: "tokyo-night",
        accent: "\x1b[38;5;81m",
        muted: "\x1b[38;5;146m",
        value: "\x1b[38;5;117m",
        title: "\x1b[1;38;5;81m",
        spectrum_hi: "\x1b[38;5;75m",
        spectrum_mid: "\x1b[38;5;81m",
        spectrum_low: "\x1b[38;5;111m",
        spectrum_spark: "\x1b[38;5;183m",
    },
    ThemeEntry {
        name: "nord",
        accent: "\x1b[38;5;110m",
        muted: "\x1b[38;5;145m",
        value: "\x1b[38;5;152m",
        title: "\x1b[1;38;5;110m",
        spectrum_hi: "\x1b[38;5;109m",
        spectrum_mid: "\x1b[38;5;110m",
        spectrum_low: "\x1b[38;5;117m",
        spectrum_spark: "\x1b[38;5;153m",
    },
    ThemeEntry {
        name: "gruvbox",
        accent: "\x1b[38;5;214m",
        muted: "\x1b[38;5;244m",
        value: "\x1b[38;5;223m",
        title: "\x1b[1;38;5;214m",
        spectrum_hi: "\x1b[38;5;208m",
        spectrum_mid: "\x1b[38;5;214m",
        spectrum_low: "\x1b[38;5;142m",
        spectrum_spark: "\x1b[38;5;180m",
    },
    ThemeEntry {
        name: "rose-pine",
        accent: "\x1b[38;5;175m",
        muted: "\x1b[38;5;145m",
        value: "\x1b[38;5;182m",
        title: "\x1b[1;38;5;175m",
        spectrum_hi: "\x1b[38;5;174m",
        spectrum_mid: "\x1b[38;5;175m",
        spectrum_low: "\x1b[38;5;146m",
        spectrum_spark: "\x1b[38;5;218m",
    },
    ThemeEntry {
        name: "catppuccin",
        accent: "\x1b[38;5;111m",
        muted: "\x1b[38;5;146m",
        value: "\x1b[38;5;189m",
        title: "\x1b[1;38;5;111m",
        spectrum_hi: "\x1b[38;5;111m",
        spectrum_mid: "\x1b[38;5;147m",
        spectrum_low: "\x1b[38;5;117m",
        spectrum_spark: "\x1b[38;5;176m",
    },
    ThemeEntry {
        name: "catppuccin-latte",
        accent: "\x1b[38;5;61m",
        muted: "\x1b[38;5;102m",
        value: "\x1b[38;5;68m",
        title: "\x1b[1;38;5;61m",
        spectrum_hi: "\x1b[38;5;61m",
        spectrum_mid: "\x1b[38;5;68m",
        spectrum_low: "\x1b[38;5;109m",
        spectrum_spark: "\x1b[38;5;132m",
    },
    ThemeEntry {
        name: "kanagawa",
        accent: "\x1b[38;5;150m",
        muted: "\x1b[38;5;145m",
        value: "\x1b[38;5;186m",
        title: "\x1b[1;38;5;150m",
        spectrum_hi: "\x1b[38;5;180m",
        spectrum_mid: "\x1b[38;5;150m",
        spectrum_low: "\x1b[38;5;109m",
        spectrum_spark: "\x1b[38;5;186m",
    },
    ThemeEntry {
        name: "everforest",
        accent: "\x1b[38;5;108m",
        muted: "\x1b[38;5;145m",
        value: "\x1b[38;5;151m",
        title: "\x1b[1;38;5;108m",
        spectrum_hi: "\x1b[38;5;143m",
        spectrum_mid: "\x1b[38;5;108m",
        spectrum_low: "\x1b[38;5;71m",
        spectrum_spark: "\x1b[38;5;180m",
    },
    ThemeEntry {
        name: "ayu-mirage-dark",
        accent: "\x1b[38;5;221m",
        muted: "\x1b[38;5;145m",
        value: "\x1b[38;5;229m",
        title: "\x1b[1;38;5;221m",
        spectrum_hi: "\x1b[38;5;215m",
        spectrum_mid: "\x1b[38;5;221m",
        spectrum_low: "\x1b[38;5;150m",
        spectrum_spark: "\x1b[38;5;222m",
    },
    ThemeEntry {
        name: "matte-black",
        accent: "\x1b[38;5;250m",
        muted: "\x1b[38;5;244m",
        value: "\x1b[38;5;255m",
        title: "\x1b[1;38;5;250m",
        spectrum_hi: "\x1b[38;5;250m",
        spectrum_mid: "\x1b[38;5;246m",
        spectrum_low: "\x1b[38;5;242m",
        spectrum_spark: "\x1b[38;5;254m",
    },
    ThemeEntry {
        name: "miasma",
        accent: "\x1b[38;5;176m",
        muted: "\x1b[38;5;139m",
        value: "\x1b[38;5;183m",
        title: "\x1b[1;38;5;176m",
        spectrum_hi: "\x1b[38;5;176m",
        spectrum_mid: "\x1b[38;5;140m",
        spectrum_low: "\x1b[38;5;109m",
        spectrum_spark: "\x1b[38;5;182m",
    },
    ThemeEntry {
        name: "osaka-jade",
        accent: "\x1b[38;5;84m",
        muted: "\x1b[38;5;116m",
        value: "\x1b[38;5;157m",
        title: "\x1b[1;38;5;84m",
        spectrum_hi: "\x1b[38;5;84m",
        spectrum_mid: "\x1b[38;5;120m",
        spectrum_low: "\x1b[38;5;157m",
        spectrum_spark: "\x1b[38;5;159m",
    },
    ThemeEntry {
        name: "ristretto",
        accent: "\x1b[38;5;180m",
        muted: "\x1b[38;5;138m",
        value: "\x1b[38;5;223m",
        title: "\x1b[1;38;5;180m",
        spectrum_hi: "\x1b[38;5;180m",
        spectrum_mid: "\x1b[38;5;179m",
        spectrum_low: "\x1b[38;5;138m",
        spectrum_spark: "\x1b[38;5;223m",
    },
    ThemeEntry {
        name: "flexoki-light",
        accent: "\x1b[38;5;137m",
        muted: "\x1b[38;5;102m",
        value: "\x1b[38;5;130m",
        title: "\x1b[1;38;5;137m",
        spectrum_hi: "\x1b[38;5;173m",
        spectrum_mid: "\x1b[38;5;137m",
        spectrum_low: "\x1b[38;5;101m",
        spectrum_spark: "\x1b[38;5;166m",
    },
    ThemeEntry {
        name: "ethereal",
        accent: "\x1b[38;5;171m",
        muted: "\x1b[38;5;146m",
        value: "\x1b[38;5;225m",
        title: "\x1b[1;38;5;171m",
        spectrum_hi: "\x1b[38;5;171m",
        spectrum_mid: "\x1b[38;5;177m",
        spectrum_low: "\x1b[38;5;153m",
        spectrum_spark: "\x1b[38;5;219m",
    },
    ThemeEntry {
        name: "hackerman",
        accent: "\x1b[38;5;46m",
        muted: "\x1b[38;5;71m",
        value: "\x1b[38;5;118m",
        title: "\x1b[1;38;5;46m",
        spectrum_hi: "\x1b[38;5;46m",
        spectrum_mid: "\x1b[38;5;82m",
        spectrum_low: "\x1b[38;5;118m",
        spectrum_spark: "\x1b[38;5;51m",
    },
    ThemeEntry {
        name: "vantablack",
        accent: "\x1b[38;5;196m",
        muted: "\x1b[38;5;244m",
        value: "\x1b[38;5;203m",
        title: "\x1b[1;38;5;196m",
        spectrum_hi: "\x1b[38;5;196m",
        spectrum_mid: "\x1b[38;5;203m",
        spectrum_low: "\x1b[38;5;210m",
        spectrum_spark: "\x1b[38;5;215m",
    },
    ThemeEntry {
        name: "Amber",
        accent: "\x1b[38;5;214m",
        muted: "\x1b[38;5;180m",
        value: "\x1b[38;5;222m",
        title: "\x1b[1;38;5;214m",
        spectrum_hi: "\x1b[38;5;208m",
        spectrum_mid: "\x1b[38;5;214m",
        spectrum_low: "\x1b[38;5;178m",
        spectrum_spark: "\x1b[38;5;221m",
    },
    ThemeEntry {
        name: "Ice",
        accent: "\x1b[38;5;117m",
        muted: "\x1b[38;5;146m",
        value: "\x1b[38;5;159m",
        title: "\x1b[1;38;5;117m",
        spectrum_hi: "\x1b[38;5;111m",
        spectrum_mid: "\x1b[38;5;117m",
        spectrum_low: "\x1b[38;5;123m",
        spectrum_spark: "\x1b[38;5;159m",
    },
];

struct KeymapEntry {
    key: &'static str,
    action_en: &'static str,
    action_zh: &'static str,
}

const KEYMAP_ENTRIES: [KeymapEntry; 30] = [
    KeymapEntry {
        key: "Space",
        action_en: "Play / Pause",
        action_zh: "播放 / 暂停",
    },
    KeymapEntry {
        key: "s",
        action_en: "Stop",
        action_zh: "停止",
    },
    KeymapEntry {
        key: "> .",
        action_en: "Next track",
        action_zh: "下一曲",
    },
    KeymapEntry {
        key: "< ,",
        action_en: "Previous track",
        action_zh: "上一曲",
    },
    KeymapEntry {
        key: "← →",
        action_en: "Seek +/-5s",
        action_zh: "快进/快退 5 秒",
    },
    KeymapEntry {
        key: "+ -",
        action_en: "Volume up/down",
        action_zh: "音量增减",
    },
    KeymapEntry {
        key: "m",
        action_en: "Toggle mono",
        action_zh: "切换单声道",
    },
    KeymapEntry {
        key: "e",
        action_en: "Cycle EQ preset",
        action_zh: "循环切换 EQ 预设",
    },
    KeymapEntry {
        key: "t",
        action_en: "Choose theme",
        action_zh: "选择主题",
    },
    KeymapEntry {
        key: "c",
        action_en: "Cycle visualizer (incl. Off)",
        action_zh: "循环切换频谱（含关闭）",
    },
    KeymapEntry {
        key: "V",
        action_en: "Full-screen visualizer",
        action_zh: "全屏频谱",
    },
    KeymapEntry {
        key: "↑ ↓",
        action_en: "Playlist scroll / EQ adjust",
        action_zh: "播放列表滚动 / EQ 调节",
    },
    KeymapEntry {
        key: "h l",
        action_en: "EQ cursor left/right",
        action_zh: "EQ 光标左/右",
    },
    KeymapEntry {
        key: "Enter",
        action_en: "Play selected track",
        action_zh: "播放选中曲目",
    },
    KeymapEntry {
        key: "a",
        action_en: "Toggle queue (play next)",
        action_zh: "加入/移出队列（下一首）",
    },
    KeymapEntry {
        key: "A",
        action_en: "Queue manager",
        action_zh: "队列管理器",
    },
    KeymapEntry {
        key: "p",
        action_en: "Playlist manager",
        action_zh: "播放列表管理器",
    },
    KeymapEntry {
        key: "i",
        action_en: "Track info / metadata",
        action_zh: "曲目信息 / 元数据",
    },
    KeymapEntry {
        key: "S",
        action_en: "Save track to ~/Music",
        action_zh: "保存曲目到 ~/Music",
    },
    KeymapEntry {
        key: "r",
        action_en: "Cycle repeat",
        action_zh: "循环模式切换",
    },
    KeymapEntry {
        key: "z",
        action_en: "Toggle shuffle",
        action_zh: "切换随机播放",
    },
    KeymapEntry {
        key: "x",
        action_en: "Expand/collapse playlist",
        action_zh: "折叠/展开播放列表",
    },
    KeymapEntry {
        key: "/",
        action_en: "Search playlist",
        action_zh: "搜索播放列表",
    },
    KeymapEntry {
        key: "u",
        action_en: "Toggle EN/ZH",
        action_zh: "切换中英文界面",
    },
    KeymapEntry {
        key: "Tab",
        action_en: "Toggle focus",
        action_zh: "切换焦点",
    },
    KeymapEntry {
        key: "Esc / b",
        action_en: "Back to provider",
        action_zh: "返回服务端播放列表",
    },
    KeymapEntry {
        key: "Ctrl+K",
        action_en: "This keymap",
        action_zh: "显示此按键说明",
    },
    KeymapEntry {
        key: "g",
        action_en: "Toggle background",
        action_zh: "切换背景动画",
    },
    KeymapEntry {
        key: "q",
        action_en: "Quit",
        action_zh: "退出",
    },
    KeymapEntry {
        key: "Ctrl+C",
        action_en: "Force quit",
        action_zh: "强制退出",
    },
];

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
    keymap_cursor: usize,
    keymap_scroll: usize,
    searching: bool,
    search_query: String,
    search_results: Vec<usize>,
    search_cursor: usize,
    prev_focus: FocusArea,
    show_themes: bool,
    theme_idx: usize,
    theme_cursor: usize,
    theme_saved_idx: usize,
    show_info: bool,
    full_vis: bool,
    show_queue: bool,
    queue_cursor: usize,
    show_pl_manager: bool,
    pl_mgr_cursor: usize,
    save_msg: Option<String>,
    save_msg_ttl: u16,
    auto_play: bool,
    bg: ParticleBackground,
    bg_enabled: bool,
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
            keymap_cursor: 0,
            keymap_scroll: 0,
            searching: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_cursor: 0,
            prev_focus: FocusArea::Playlist,
            show_themes: false,
            theme_idx: 0,
            theme_cursor: 0,
            theme_saved_idx: 0,
            show_info: false,
            full_vis: false,
            show_queue: false,
            queue_cursor: 0,
            show_pl_manager: false,
            pl_mgr_cursor: 0,
            save_msg: None,
            save_msg_ttl: 0,
            auto_play: false,
            bg: ParticleBackground::new(PANEL_WIDTH, 24),
            bg_enabled: true,
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

    pub fn set_theme_by_name(&mut self, name: &str) -> bool {
        let normalized = normalize_theme_name(name);
        if normalized.is_empty() || normalized == "default" || normalized == "neo" {
            self.theme_idx = 0;
            return true;
        }
        for (idx, theme) in THEMES.iter().enumerate() {
            if normalize_theme_name(theme.name) == normalized {
                self.theme_idx = idx;
                return true;
            }
        }
        match normalized.as_str() {
            "neon" | "neo-mint" | "neo_mint" => {
                self.theme_idx = 0;
                true
            }
            "tokyonight" => self.set_theme_by_name("tokyo-night"),
            _ => false,
        }
    }

    pub fn set_auto_play(&mut self, enabled: bool) {
        self.auto_play = enabled;
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

    fn toggle_background(&mut self) {
        self.bg_enabled = !self.bg_enabled;
    }

    fn theme_name(&self) -> &str {
        self.current_theme().name
    }

    fn current_theme(&self) -> &ThemeEntry {
        THEMES.get(self.theme_idx).unwrap_or(&THEMES[0])
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
                self.player.stop();
                self.player.clear_preload();
                self.playlist.replace(tracks);
                self.pl_cursor = 0;
                self.pl_scroll = 0;
                self.focus = FocusArea::Playlist;
                if self.playlist.len() > 0 {
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
        let mut last_tick = Instant::now();

        if self.auto_play && self.playlist.len() > 0 {
            self.focus = FocusArea::Playlist;
            self.playlist
                .set_index(self.pl_cursor.min(self.playlist.len() - 1));
            self.play_current_track();
            self.auto_play = false;
        }

        loop {
            self.draw(stdout)?;

            let tick_rate = self.tick_rate();
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
        let frame_plain = self.render();
        let scene = if let Ok((w, h)) = terminal::size() {
            let term_w = w as usize;
            let term_h = h as usize;
            self.bg.resize(term_w, term_h);
            self.compose_scene(&frame_plain, term_w, term_h)
        } else {
            self.render_fallback_scene(&frame_plain)
        };
        let output = scene.replace('\n', "\r\n");

        stdout.execute(cursor::MoveTo(0, 0))?;
        stdout.execute(terminal::Clear(ClearType::All))?;
        stdout.execute(Print(output))?;
        stdout.flush()?;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit();
            return;
        }

        if self.show_keymap {
            self.handle_keymap_key(key);
            return;
        }

        if self.show_themes {
            self.handle_theme_key(key);
            return;
        }

        if self.show_pl_manager {
            self.handle_playlist_manager_key(key);
            return;
        }

        if self.show_queue {
            self.handle_queue_key(key);
            return;
        }

        if self.show_info {
            self.handle_info_key(key);
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
            self.keymap_cursor = 0;
            self.keymap_scroll = 0;
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
                if self.full_vis {
                    self.full_vis = false;
                } else if self.provider.is_some() {
                    self.focus = FocusArea::Provider;
                }
            }
            KeyCode::Char(' ') => {
                if self.player.is_loading() {
                    return;
                }
                if !self.player.is_playing() {
                    self.play_current_track();
                } else {
                    self.player.toggle_pause();
                }
            }
            KeyCode::Char('s') => self.player.stop(),
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
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.playlist.cycle_repeat();
                self.player.clear_preload();
                self.preload_next();
            }
            KeyCode::Char('z') | KeyCode::Char('Z') => {
                self.playlist.toggle_shuffle();
                self.player.clear_preload();
                self.preload_next();
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                self.pl_visible = if self.pl_visible == 5 {
                    EXPANDED_VIS_ROWS
                } else {
                    5
                };
                self.adjust_scroll();
            }
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
            KeyCode::Char('c') | KeyCode::Char('C') => self.vis.cycle_mode(),
            KeyCode::Char('V') => self.full_vis = !self.full_vis,
            KeyCode::Char('m') | KeyCode::Char('M') => self.player.toggle_mono(),
            KeyCode::Char('a') => {
                if self.focus == FocusArea::Playlist && self.pl_cursor < self.playlist.len() {
                    if !self.playlist.dequeue(self.pl_cursor) {
                        self.playlist.queue(self.pl_cursor);
                    }
                    self.player.clear_preload();
                    self.preload_next();
                }
            }
            KeyCode::Char('A') => {
                self.show_queue = true;
                self.queue_cursor = 0;
            }
            KeyCode::Char('p') => {
                self.show_pl_manager = true;
                self.pl_mgr_cursor = self.pl_cursor.min(self.playlist.len().saturating_sub(1));
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                self.show_info = true;
            }
            KeyCode::Char('u') | KeyCode::Char('U') => self.toggle_language(),
            KeyCode::Char('t') | KeyCode::Char('T') => {
                self.show_themes = true;
                self.theme_saved_idx = self.theme_idx;
                self.theme_cursor = self.theme_idx;
            }
            KeyCode::Char('g') | KeyCode::Char('G') => self.toggle_background(),
            KeyCode::Char('S') => self.save_current_track(),
            KeyCode::Char('/') => self.start_search(),
            _ => {}
        }
    }

    fn handle_keymap_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('k') | KeyCode::Char('K'))
        {
            self.show_keymap = false;
            return;
        }

        let max_visible = 14usize;
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.show_keymap = false;
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                if self.keymap_cursor > 0 {
                    self.keymap_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                if self.keymap_cursor + 1 < KEYMAP_ENTRIES.len() {
                    self.keymap_cursor += 1;
                }
            }
            _ => {}
        }

        if self.keymap_cursor < self.keymap_scroll {
            self.keymap_scroll = self.keymap_cursor;
        }
        if self.keymap_cursor >= self.keymap_scroll + max_visible {
            self.keymap_scroll = self.keymap_cursor + 1 - max_visible;
        }
    }

    fn handle_theme_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                if self.theme_cursor > 0 {
                    self.theme_cursor -= 1;
                    self.theme_idx = self.theme_cursor;
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                if self.theme_cursor + 1 < THEMES.len() {
                    self.theme_cursor += 1;
                    self.theme_idx = self.theme_cursor;
                }
            }
            KeyCode::Enter => {
                self.theme_idx = self.theme_cursor;
                self.show_themes = false;
            }
            KeyCode::Esc | KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::Char('q') => {
                self.theme_idx = self.theme_saved_idx;
                self.show_themes = false;
            }
            _ => {}
        }
    }

    fn handle_info_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Char('q') => {
                self.show_info = false;
            }
            _ => {}
        }
    }

    fn handle_queue_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('A') => self.show_queue = false,
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                if self.queue_cursor > 0 {
                    self.queue_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                if self.queue_cursor + 1 < self.playlist.queue_len() {
                    self.queue_cursor += 1;
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.playlist.remove_queue_at(self.queue_cursor) {
                    if self.queue_cursor >= self.playlist.queue_len() && self.queue_cursor > 0 {
                        self.queue_cursor -= 1;
                    }
                    self.player.clear_preload();
                    self.preload_next();
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.playlist.clear_queue();
                self.queue_cursor = 0;
                self.player.clear_preload();
                self.preload_next();
            }
            _ => {}
        }
    }

    fn handle_playlist_manager_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('p') => self.show_pl_manager = false,
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                if self.pl_mgr_cursor > 0 {
                    self.pl_mgr_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                if self.pl_mgr_cursor + 1 < self.playlist.len() {
                    self.pl_mgr_cursor += 1;
                }
            }
            KeyCode::Enter => {
                if self.playlist.len() > 0 {
                    self.pl_cursor = self.pl_mgr_cursor.min(self.playlist.len() - 1);
                    self.playlist.set_index(self.pl_cursor);
                    self.adjust_scroll();
                    self.play_current_track();
                    self.show_pl_manager = false;
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.remove_playlist_track(self.pl_mgr_cursor)
            }
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
            KeyCode::Char(' ') => {
                if self.player.is_loading() {
                    return;
                }
                if !self.player.is_playing() {
                    self.play_current_track();
                } else {
                    self.player.toggle_pause();
                }
            }
            KeyCode::Char('u') | KeyCode::Char('U') => self.toggle_language(),
            KeyCode::Char('t') | KeyCode::Char('T') => {
                self.show_themes = true;
                self.theme_saved_idx = self.theme_idx;
                self.theme_cursor = self.theme_idx;
            }
            KeyCode::Char('i') | KeyCode::Char('I') => self.show_info = true,
            KeyCode::Char('g') | KeyCode::Char('G') => self.toggle_background(),
            KeyCode::Char('A') => {
                self.show_queue = true;
                self.queue_cursor = 0;
            }
            KeyCode::Char('p') => {
                if self.playlist.len() > 0 {
                    self.show_pl_manager = true;
                    self.pl_mgr_cursor = self.pl_cursor.min(self.playlist.len().saturating_sub(1));
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
        if let Some(err) = self.player.take_error() {
            self.error = Some(err);
        }
        if self.player.take_gapless_advanced() {
            let _ = self.playlist.next();
            if let Some(idx) = self.playlist.index() {
                self.pl_cursor = idx;
                self.adjust_scroll();
            }
            self.title_off = 0;
            self.error = None;
            self.preload_next();
        }
        if self.player.is_playing() && !self.player.is_paused() && self.player.track_done() {
            self.next_track();
        }
        self.title_off = self.title_off.wrapping_add(1);
        if self.bg_enabled {
            self.bg.tick();
        }
        if self.save_msg_ttl > 0 {
            self.save_msg_ttl -= 1;
            if self.save_msg_ttl == 0 {
                self.save_msg = None;
            }
        }
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
                self.play_track(track, idx);
            }
        } else {
            if let Some((track, idx)) = self.playlist.current() {
                if track.stream {
                    self.play_track(track, idx);
                    return;
                }
            }
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
                self.play_track(track, idx);
            }
        }
    }

    fn play_current_track(&mut self) {
        if let Some((track, idx)) = self.playlist.current() {
            self.play_track(track, idx);
        }
    }

    fn play_track(&mut self, mut track: crate::playlist::Track, track_idx: usize) {
        self.title_off = 0;

        if track.ytdlp {
            match ytdlp::resolve_stream_url(&track.path) {
                Ok(stream_url) => {
                    track.path = stream_url;
                    track.ytdlp = false;
                    self.playlist.set_track(track_idx, track.clone());
                }
                Err(err) => {
                    self.error = Some(err.to_string());
                    return;
                }
            }
        }

        self.player.play_async(&track.path);
        self.error = None;
        self.preload_next();
    }

    fn preload_next(&mut self) {
        let Some(next) = self.playlist.peek_next() else {
            self.player.clear_preload();
            return;
        };

        if next.stream || next.ytdlp {
            self.player.clear_preload();
            return;
        }

        self.player.preload_async(&next.path);
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

    fn remove_playlist_track(&mut self, idx: usize) {
        if idx >= self.playlist.len() {
            return;
        }

        let was_playing = self.player.is_playing();
        let removed_current = self.playlist.index() == Some(idx);
        if !self.playlist.remove_at(idx) {
            return;
        }

        if self.playlist.len() == 0 {
            self.player.stop();
            self.pl_cursor = 0;
            self.pl_scroll = 0;
            self.pl_mgr_cursor = 0;
            return;
        }

        self.pl_cursor = self.pl_cursor.min(self.playlist.len() - 1);
        self.pl_mgr_cursor = self.pl_mgr_cursor.min(self.playlist.len() - 1);
        self.adjust_scroll();

        if removed_current && was_playing {
            let next_idx = idx.min(self.playlist.len() - 1);
            self.playlist.set_index(next_idx);
            self.pl_cursor = next_idx;
            self.play_current_track();
        } else {
            self.player.clear_preload();
            self.preload_next();
        }
    }

    fn save_current_track(&mut self) {
        let Some((track, _)) = self.playlist.current() else {
            self.save_msg = Some(self.tr("Nothing to save", "没有可保存的曲目").to_string());
            self.save_msg_ttl = 60;
            return;
        };

        if track.stream || track.path.starts_with("http://") || track.path.starts_with("https://") {
            self.save_msg = Some(
                self.tr(
                    "Only local file tracks can be saved",
                    "仅支持保存本地文件曲目",
                )
                .to_string(),
            );
            self.save_msg_ttl = 60;
            return;
        }

        let src = Path::new(&track.path);
        if !src.exists() {
            self.save_msg = Some(self.tr("Source file not found", "源文件不存在").to_string());
            self.save_msg_ttl = 60;
            return;
        }

        let home = env::var_os("HOME");
        let Some(home) = home else {
            self.save_msg = Some(
                self.tr("HOME is not set", "HOME 环境变量未设置")
                    .to_string(),
            );
            self.save_msg_ttl = 60;
            return;
        };

        let target_dir = Path::new(&home).join("Music");
        if let Err(err) = fs::create_dir_all(&target_dir) {
            self.save_msg = Some(format!("{}: {err}", self.tr("Save failed", "保存失败")));
            self.save_msg_ttl = 60;
            return;
        }

        let ext = src.extension().and_then(|v| v.to_str()).unwrap_or("mp3");
        let mut base = if track.artist.is_empty() {
            track.title.clone()
        } else {
            format!("{} - {}", track.artist, track.title)
        };
        if base.trim().is_empty() {
            base = "rliamp-track".to_string();
        }

        let mut candidate = target_dir.join(format!("{}.{}", sanitize_filename(&base), ext));
        let mut n = 1usize;
        while candidate.exists() {
            candidate = target_dir.join(format!("{}-{}.{}", sanitize_filename(&base), n, ext));
            n += 1;
        }

        match fs::copy(src, &candidate) {
            Ok(_) => {
                self.save_msg = Some(format!(
                    "{} {}",
                    self.tr("Saved:", "已保存:"),
                    candidate.display()
                ));
            }
            Err(err) => {
                self.save_msg = Some(format!("{}: {err}", self.tr("Save failed", "保存失败")));
            }
        }
        self.save_msg_ttl = 80;
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

    fn tick_rate(&self) -> Duration {
        if self.vis.is_disabled() && !self.full_vis {
            Duration::from_millis(TICK_MS_VIS_OFF)
        } else {
            Duration::from_millis(TICK_MS_ACTIVE)
        }
    }

    fn render(&mut self) -> String {
        if self.show_keymap {
            return wrap_frame(self.render_keymap());
        }
        if self.show_themes {
            return wrap_frame(self.render_theme_picker());
        }
        if self.show_pl_manager {
            return wrap_frame(self.render_playlist_manager());
        }
        if self.show_queue {
            return wrap_frame(self.render_queue_manager());
        }
        if self.show_info {
            return wrap_frame(self.render_track_info_overlay());
        }
        if self.full_vis {
            return wrap_frame(self.render_full_visualizer());
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
        if let Some(msg) = &self.save_msg {
            lines.push(msg.clone());
        }

        wrap_frame(lines)
    }

    fn render_keymap(&self) -> Vec<String> {
        let mut lines = vec![
            self.tr("K E Y M A P", "按 键 说 明").to_string(),
            String::new(),
        ];
        let max_visible = 14usize;
        let start = self
            .keymap_scroll
            .min(KEYMAP_ENTRIES.len().saturating_sub(1));
        let end = (start + max_visible).min(KEYMAP_ENTRIES.len());

        for (idx, entry) in KEYMAP_ENTRIES.iter().enumerate().take(end).skip(start) {
            let action = self.tr(entry.action_en, entry.action_zh);
            let label = format!("{:<10} {}", entry.key, action);
            if idx == self.keymap_cursor {
                lines.push(format!("> {label}"));
            } else {
                lines.push(format!("  {label}"));
            }
        }
        while lines.len() < max_visible + 2 {
            lines.push(String::new());
        }
        lines.push(String::new());
        lines.push(format!(
            "  {}/{}",
            self.keymap_cursor + 1,
            KEYMAP_ENTRIES.len()
        ));
        lines.push(
            self.tr("[↑↓]Navigate [Esc]Close", "[↑↓]移动 [Esc]关闭")
                .to_string(),
        );

        lines
    }

    fn render_theme_picker(&self) -> Vec<String> {
        let mut lines = vec![self.tr("T H E M E S", "主 题").to_string(), String::new()];
        let max_visible = 14usize;
        let scroll = self
            .theme_cursor
            .saturating_sub(max_visible.saturating_sub(1));
        for (idx, theme) in THEMES.iter().enumerate().skip(scroll).take(max_visible) {
            let prefix = if idx == self.theme_cursor { "> " } else { "  " };
            let marker = if idx == self.theme_idx { "*" } else { " " };
            lines.push(format!("{prefix}[{marker}] {}", theme.name));
        }
        if THEMES.len() > max_visible {
            lines.push(String::new());
            lines.push(format!("  {}/{}", self.theme_cursor + 1, THEMES.len()));
        }
        lines.push(String::new());
        lines.push(
            self.tr(
                "[↑↓]Preview [Enter]Select [Esc/t]Cancel",
                "[↑↓]预览 [Enter]选择 [Esc/t]取消",
            )
            .to_string(),
        );
        lines
    }

    fn render_queue_manager(&self) -> Vec<String> {
        let mut lines = vec![self.tr("Q U E U E", "队 列").to_string(), String::new()];
        let tracks = self.playlist.queued_tracks();

        if tracks.is_empty() {
            lines.push(self.tr("  (empty)", "  （空）").to_string());
        } else {
            let max_visible = self.pl_visible.max(8);
            let scroll = self
                .queue_cursor
                .saturating_sub(max_visible.saturating_sub(1));
            for (i, track) in tracks.iter().enumerate().skip(scroll).take(max_visible) {
                let mut name = track.display_name();
                if display_width(&name) > PANEL_WIDTH.saturating_sub(8) {
                    let mut trimmed = truncate_to_width(&name, PANEL_WIDTH.saturating_sub(9));
                    trimmed.push('…');
                    name = trimmed;
                }
                if i == self.queue_cursor {
                    lines.push(format!("> {}. {name}", i + 1));
                } else {
                    lines.push(format!("  {}. {name}", i + 1));
                }
            }
        }

        lines.push(String::new());
        lines.push(
            self.tr(
                "[↑↓]Navigate [d]Remove [c]Clear [Esc/A]Close",
                "[↑↓]移动 [d]移除 [c]清空 [Esc/A]关闭",
            )
            .to_string(),
        );
        lines
    }

    fn render_playlist_manager(&self) -> Vec<String> {
        let mut lines = vec![
            self.tr("P L A Y L I S T  M A N A G E R", "播 放 列 表 管 理")
                .to_string(),
            String::new(),
        ];

        if self.playlist.len() == 0 {
            lines.push(
                self.tr("  No tracks loaded", "  没有可播放曲目")
                    .to_string(),
            );
            lines.push(String::new());
            lines.push(self.tr("[Esc/p]Close", "[Esc/p]关闭").to_string());
            return lines;
        }

        let tracks = self.playlist.tracks();
        let max_visible = self.pl_visible.max(10);
        let scroll = self
            .pl_mgr_cursor
            .saturating_sub(max_visible.saturating_sub(1));
        for (idx, track) in tracks.iter().enumerate().skip(scroll).take(max_visible) {
            let mut name = track.display_name();
            if display_width(&name) > PANEL_WIDTH.saturating_sub(8) {
                let mut trimmed = truncate_to_width(&name, PANEL_WIDTH.saturating_sub(9));
                trimmed.push('…');
                name = trimmed;
            }
            if idx == self.pl_mgr_cursor {
                lines.push(format!("> {}. {name}", idx + 1));
            } else {
                lines.push(format!("  {}. {name}", idx + 1));
            }
        }

        lines.push(String::new());
        lines.push(
            self.tr(
                "[↑↓]Navigate [Enter]Play [d]Remove [Esc/p]Close",
                "[↑↓]移动 [Enter]播放 [d]删除 [Esc/p]关闭",
            )
            .to_string(),
        );
        lines
    }

    fn render_track_info_overlay(&self) -> Vec<String> {
        let mut lines = vec![
            self.tr("T R A C K  I N F O", "曲 目 信 息").to_string(),
            String::new(),
        ];
        let Some((track, _)) = self.playlist.current() else {
            lines.push(self.tr("  No track loaded", "  未加载曲目").to_string());
            lines.push(String::new());
            lines.push(self.tr("[Esc/i]Close", "[Esc/i]关闭").to_string());
            return lines;
        };

        lines.push(format!("  {}: {}", self.tr("Title", "标题"), track.title));
        lines.push(format!(
            "  {}: {}",
            self.tr("Artist", "艺术家"),
            if track.artist.is_empty() {
                self.tr("(unknown)", "（未知）").to_string()
            } else {
                track.artist
            }
        ));
        lines.push(format!(
            "  {}: {}",
            self.tr("Album", "专辑"),
            self.tr("(unknown)", "（未知）")
        ));
        lines.push(format!("  {}: {}", self.tr("Path", "路径"), track.path));
        lines.push(String::new());
        lines.push(self.tr("[Esc/i]Close", "[Esc/i]关闭").to_string());
        lines
    }

    fn render_full_visualizer(&mut self) -> Vec<String> {
        let mut lines = vec![
            self.render_title(),
            self.render_track_info(),
            self.render_time_status(),
            String::new(),
        ];
        lines.extend(self.render_spectrum());
        lines.push(self.render_seek_bar());
        lines.push(String::new());
        lines.push(
            self.tr(
                "[V/Esc]Exit full visualizer [c]Mode [t]Theme",
                "[V/Esc]退出全屏 [c]切换频谱 [t]主题",
            )
            .to_string(),
        );
        if let Some(msg) = &self.save_msg {
            lines.push(msg.clone());
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
        } else if self.player.is_loading() {
            self.tr("… Loading", "… 载入中")
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
        let rows = if self.full_vis {
            self.full_vis_rows()
        } else {
            DEFAULT_VIS_ROWS
        };
        self.vis.set_rows(rows);
        if self.vis.is_disabled() {
            let bands = self.vis.analyze(&[]);
            return self.vis.render(bands, self.title_off as u64);
        }
        let bands = self.vis.analyze(&self.player.samples(2048));
        self.vis.render(bands, self.title_off as u64)
    }

    fn full_vis_rows(&self) -> usize {
        if let Ok((_w, h)) = terminal::size() {
            return ((h as usize).saturating_sub(11)).clamp(8, 30);
        }
        16
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

        let vis = if self.lang == UiLang::Zh {
            format!(" [可视化: {}]", self.vis.mode_name())
        } else {
            format!(" [Vis: {}]", self.vis.mode_name())
        };
        let theme = if self.lang == UiLang::Zh {
            format!(" [主题: {}]", self.theme_name())
        } else {
            format!(" [Theme: {}]", self.theme_name())
        };

        format!(
            "── {} ── {shuffle} {repeat}{queue}{vis}{theme} ──",
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
                    "[↑↓]Navigate [Enter]Load [u]Lang [i]Info [t]Theme [Tab]Focus [Q]Quit",
                    "[↑↓]移动 [Enter]加载 [u]语言 [i]信息 [t]主题 [Tab]焦点 [Q]退出",
                )
                .to_string()];
        }

        let mut line1 = String::from(self.tr("[Spc]⏯ [<>]Trk ", "[空格]⏯ [<>]曲目 "));
        if !self.current_track_is_stream() {
            line1.push_str(self.tr("[←→]Seek ", "[←→]快进/退 "));
        }
        line1.push_str(self.tr(
            "[+-]Vol [m]Mono [e]EQ [c]Vis [V]Full [t]Theme [u]Lang [i]Info",
            "[+-]音量 [m]单声道 [e]EQ [c]频谱 [V]全屏 [t]主题 [u]语言 [i]信息",
        ));

        let mut line2 = String::new();
        if self.provider.is_some() {
            line2.push_str(self.tr("[Esc]Back ", "[Esc]返回 "));
        }
        line2.push_str(self.tr(
            "[g]BG [a]Queue [A]QueueMgr [p]PlMgr [S]Save [x]Expand [/]Search [Tab]Focus [Q]Quit",
            "[g]背景 [a]队列 [A]队列管理 [p]列表管理 [S]保存 [x]展开 [/]搜索 [Tab]焦点 [Q]退出",
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

    fn compose_scene(&self, frame_plain: &str, term_w: usize, term_h: usize) -> String {
        if term_w == 0 || term_h == 0 {
            return self.render_fallback_scene(frame_plain);
        }

        let frame_lines: Vec<&str> = frame_plain.lines().collect();
        let frame_h = frame_lines.len();
        let frame_w = frame_lines
            .iter()
            .map(|line| display_width(line))
            .max()
            .unwrap_or(PANEL_WIDTH + 6);

        let pad_left = term_w.saturating_sub(frame_w) / 2;
        let pad_top = term_h.saturating_sub(frame_h) / 2;

        let mut out = Vec::with_capacity(term_h);
        for y in 0..term_h {
            let bg_line = self.render_background_line(y, term_w);

            if y >= pad_top && y < pad_top.saturating_add(frame_h) {
                let frame_line = frame_lines[y - pad_top];
                let left_bg: String = bg_line.chars().take(pad_left).collect();
                let right_bg: String = bg_line.chars().skip(pad_left + frame_w).collect();

                let mut line = String::new();
                line.push_str(&self.colorize_background_line(&left_bg));
                line.push_str(&self.colorize_frame_line(frame_line));
                line.push_str(&self.colorize_background_line(&right_bg));
                out.push(line);
            } else {
                out.push(self.colorize_background_line(&bg_line));
            }
        }

        out.join("\n")
    }

    fn render_fallback_scene(&self, frame_plain: &str) -> String {
        self.colorize_frame(frame_plain)
    }

    fn render_background_line(&self, y: usize, width: usize) -> String {
        if !self.bg_enabled {
            return " ".repeat(width);
        }
        let mut line = String::with_capacity(width);
        for x in 0..width {
            line.push(self.bg.ch_at(x, y));
        }
        line
    }

    fn colorize_frame_line(&self, line: &str) -> String {
        if colors_enabled() {
            self.colorize_line(line)
        } else {
            line.to_string()
        }
    }

    fn colorize_background_line(&self, line: &str) -> String {
        if colors_enabled() {
            colorize_particle_line(line)
        } else {
            line.to_string()
        }
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
        let theme = self.current_theme();

        if trimmed.starts_with("C L I A M P")
            || trimmed.starts_with("K E Y M A P")
            || trimmed.starts_with("按 键 说 明")
            || trimmed.starts_with("T R A C K  I N F O")
            || trimmed.starts_with("曲 目 信 息")
            || trimmed.starts_with("T H E M E S")
            || trimmed.starts_with("主 题")
        {
            return paint(theme.title, content);
        }

        if is_track_info_kv_line(trimmed) {
            return self.colorize_track_info_line(content);
        }

        if trimmed.starts_with("♫ ") {
            return paint(theme.value, content);
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
                    ("[Shuffle*]", theme.accent),
                    ("[随机*]", theme.accent),
                    ("[Repeat: All]", theme.accent),
                    ("[循环: 全部]", theme.accent),
                    ("[Repeat: One]", theme.accent),
                    ("[循环: 单曲]", theme.accent),
                    ("[Queue:", theme.accent),
                    ("[队列:", theme.accent),
                    ("[Vis:", theme.accent),
                    ("[可视化:", theme.accent),
                    ("[Theme:", theme.accent),
                    ("[主题:", theme.accent),
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

        if is_shortcut_hint_line(trimmed)
            || trimmed.starts_with('/')
            || trimmed.starts_with("Press ")
            || trimmed.starts_with("按任意键")
        {
            return self.colorize_shortcut_line(content);
        }

        if is_streaming_seek_line(trimmed) {
            return paint(ANSI_YELLOW, content);
        }

        if is_spectrum_line(trimmed) {
            return self.colorize_spectrum_line(content);
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
                ("… Loading", ANSI_YELLOW_BOLD),
                ("… 载入中", ANSI_YELLOW_BOLD),
                ("⏸ Paused", ANSI_YELLOW_BOLD),
                ("⏸ 暂停", ANSI_YELLOW_BOLD),
                ("■ Stopped", ANSI_DIM),
                ("■ 已停止", ANSI_DIM),
                ("[Q", theme.accent),
            ],
        );
        styled = styled.replace("[Q", &format!("{}[Q{ANSI_TEXT}", theme.accent));
        styled
    }

    fn colorize_spectrum_line(&self, content: &str) -> String {
        let theme = self.current_theme();

        let mut out = String::new();
        for ch in content.chars() {
            match ch {
                '█' | '▉' | '▊' | '▇' | '▆' => {
                    out.push_str(&paint(theme.spectrum_hi, &ch.to_string()))
                }
                '▓' | '▅' | '▄' => out.push_str(&paint(theme.spectrum_mid, &ch.to_string())),
                '▒' | '░' | '▃' | '▂' | '▁' => {
                    out.push_str(&paint(theme.spectrum_low, &ch.to_string()))
                }
                '\u{2800}'..='\u{28FF}' => {
                    out.push_str(&paint(theme.spectrum_low, &ch.to_string()))
                }
                '✦' | '•' | '·' | '.' => {
                    out.push_str(&paint(theme.spectrum_spark, &ch.to_string()))
                }
                ' ' => out.push(' '),
                _ => out.push(ch),
            }
        }
        out
    }

    fn colorize_shortcut_line(&self, content: &str) -> String {
        let theme = self.current_theme();
        let mut out = String::new();
        let mut in_bracket = false;

        for ch in content.chars() {
            match ch {
                '[' => {
                    in_bracket = true;
                    out.push_str(&paint(theme.muted, "["));
                }
                ']' => {
                    in_bracket = false;
                    out.push_str(&paint(theme.muted, "]"));
                }
                ' ' => out.push(' '),
                _ => {
                    if in_bracket {
                        out.push_str(&paint(theme.accent, &ch.to_string()));
                    } else {
                        out.push_str(&paint(theme.muted, &ch.to_string()));
                    }
                }
            }
        }
        out
    }

    fn colorize_track_info_line(&self, content: &str) -> String {
        let theme = self.current_theme();
        if let Some(idx) = content.find(':') {
            let (label, rest) = content.split_at(idx + 1);
            return format!("{}{}", paint(theme.muted, label), paint(theme.value, rest));
        }
        paint(theme.value, content)
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
        if ('\u{2800}'..='\u{28FF}').contains(&ch) {
            has_bar = true;
            continue;
        }
        return false;
    }
    has_bar
}

fn is_track_info_kv_line(trimmed: &str) -> bool {
    const LABELS: [&str; 12] = [
        "Title:",
        "Artist:",
        "Album:",
        "Path:",
        "Stream:",
        "Duration:",
        "标题:",
        "艺术家:",
        "专辑:",
        "路径:",
        "流媒体:",
        "时长:",
    ];
    LABELS.iter().any(|label| trimmed.starts_with(label))
}

fn is_shortcut_hint_line(trimmed: &str) -> bool {
    if trimmed.starts_with("── ") {
        return false;
    }
    let bracketed = trimmed.contains('[') && trimmed.contains(']');
    bracketed
        && (trimmed.starts_with('[')
            || trimmed.starts_with("> [")
            || trimmed.starts_with("  [")
            || trimmed.contains(" ["))
}

fn normalize_theme_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-")
}

fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            out.push('_');
        } else if ch.is_control() {
            continue;
        } else {
            out.push(ch);
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "rliamp-track".to_string()
    } else {
        trimmed.to_string()
    }
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
