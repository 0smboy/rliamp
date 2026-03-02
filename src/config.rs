use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub volume: f32,
    pub eq: [f32; 10],
    pub eq_preset: String,
    pub theme: Option<String>,
    pub repeat: String,
    pub shuffle: bool,
    pub mono: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 0.0,
            eq: [0.0; 10],
            eq_preset: "Flat".to_string(),
            theme: None,
            repeat: "off".to_string(),
            shuffle: false,
            mono: false,
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

        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();

            match key {
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
                "eq" => cfg.eq = parse_eq(value),
                _ => {}
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
