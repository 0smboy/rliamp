use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct NavidromeConfig {
    pub url: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct YtMusicConfig {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub cookies_from: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SpotifyConfig {
    pub client_id: Option<String>,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub volume: f32,
    pub eq: [f32; 10],
    pub eq_preset: String,
    pub theme: Option<String>,
    pub visualizer: Option<String>,
    pub compact: bool,
    pub provider: String,
    pub repeat: String,
    pub shuffle: bool,
    pub mono: bool,
    pub sample_rate: Option<u32>,
    pub buffer_ms: Option<u32>,
    pub resample_quality: u8,
    pub bit_depth: u16,
    pub seek_large_step_sec: u64,
    pub navidrome: NavidromeConfig,
    pub ytmusic: YtMusicConfig,
    pub spotify: SpotifyConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 0.0,
            eq: [0.0; 10],
            eq_preset: "Flat".to_string(),
            theme: None,
            visualizer: None,
            compact: false,
            provider: "radio".to_string(),
            repeat: "off".to_string(),
            shuffle: false,
            mono: false,
            sample_rate: None,
            buffer_ms: None,
            resample_quality: 2,
            bit_depth: 32,
            seek_large_step_sec: 30,
            navidrome: NavidromeConfig::default(),
            ytmusic: YtMusicConfig::default(),
            spotify: SpotifyConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> io::Result<Self> {
        let mut cfg = Config::default();
        let path = config_path()?;

        let Ok(content) = fs::read_to_string(path) else {
            return Ok(cfg);
        };

        let mut section = String::new();
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();

            match section.as_str() {
                "navidrome" => match key {
                    "url" => cfg.navidrome.url = parse_optional_string(value),
                    "user" | "username" => cfg.navidrome.user = parse_optional_string(value),
                    "password" | "pass" => cfg.navidrome.password = parse_optional_string(value),
                    "token" => cfg.navidrome.token = parse_optional_string(value),
                    _ => {}
                },
                "ytmusic" | "youtube" => match key {
                    "client_id" => cfg.ytmusic.client_id = parse_optional_string(value),
                    "client_secret" => cfg.ytmusic.client_secret = parse_optional_string(value),
                    "cookies_from" => cfg.ytmusic.cookies_from = parse_optional_string(value),
                    _ => {}
                },
                "spotify" => match key {
                    "client_id" => cfg.spotify.client_id = parse_optional_string(value),
                    "redirect_uri" => cfg.spotify.redirect_uri = parse_optional_string(value),
                    _ => {}
                },
                _ => match key {
                    "volume" => {
                        if let Ok(v) = value.parse::<f32>() {
                            cfg.volume = v.clamp(-30.0, 6.0);
                        }
                    }
                    "repeat" => {
                        let v = trim_quotes(value).to_ascii_lowercase();
                        if matches!(v.as_str(), "off" | "all" | "one") {
                            cfg.repeat = v;
                        }
                    }
                    "shuffle" => cfg.shuffle = value.eq_ignore_ascii_case("true"),
                    "mono" => cfg.mono = value.eq_ignore_ascii_case("true"),
                    "eq_preset" => cfg.eq_preset = trim_quotes(value).to_string(),
                    "theme" => {
                        let v = trim_quotes(value).trim();
                        if !v.is_empty() {
                            cfg.theme = Some(v.to_string());
                        }
                    }
                    "provider" => {
                        let v = trim_quotes(value).trim().to_ascii_lowercase();
                        if matches!(
                            v.as_str(),
                            "radio"
                                | "navidrome"
                                | "youtube"
                                | "yt"
                                | "ytmusic"
                                | "spotify"
                                | "none"
                        ) {
                            cfg.provider = v;
                        }
                    }
                    "visualizer" => {
                        let v = trim_quotes(value).trim();
                        if !v.is_empty() {
                            cfg.visualizer = Some(v.to_string());
                        }
                    }
                    "compact" => cfg.compact = value.eq_ignore_ascii_case("true"),
                    "sample_rate" => {
                        if let Ok(v) = value.parse::<u32>() {
                            cfg.sample_rate = Some(v.clamp(8_000, 384_000));
                        }
                    }
                    "buffer_ms" => {
                        if let Ok(v) = value.parse::<u32>() {
                            cfg.buffer_ms = Some(v.clamp(20, 2_000));
                        }
                    }
                    "resample_quality" => {
                        if let Ok(v) = value.parse::<u8>() {
                            cfg.resample_quality = v.clamp(1, 4);
                        }
                    }
                    "bit_depth" => {
                        if let Ok(v) = value.parse::<u16>() {
                            cfg.bit_depth = if v <= 16 { 16 } else { 32 };
                        }
                    }
                    "seek_large_step_sec" | "seek_step_large" => {
                        if let Ok(v) = value.parse::<u64>() {
                            cfg.seek_large_step_sec = v.clamp(1, 600);
                        }
                    }
                    "eq" => cfg.eq = parse_eq(value),
                    _ => {}
                },
            }
        }

        Ok(cfg)
    }
}

fn config_path() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME not set"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("rliamp")
        .join("config.toml"))
}

fn trim_quotes(s: &str) -> &str {
    s.trim_matches('"').trim_matches('\'')
}

fn parse_optional_string(value: &str) -> Option<String> {
    let v = trim_quotes(value).trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

fn parse_eq(value: &str) -> [f32; 10] {
    let mut out = [0.0; 10];
    let body = value.trim().trim_start_matches('[').trim_end_matches(']');
    for (i, item) in body.split(',').enumerate() {
        if i >= 10 {
            break;
        }
        if let Ok(v) = item.trim().parse::<f32>() {
            out[i] = v.clamp(-12.0, 12.0);
        }
    }
    out
}
