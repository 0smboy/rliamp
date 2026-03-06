use serde::Deserialize;
use std::fmt;
use std::time::Duration;

const REQUEST_TIMEOUT_SECS: u64 = 10;
const MAX_RESPONSE_BODY: usize = 2 << 20;

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub start: Duration,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum LyricsError {
    NotFound,
    Message(String),
}

impl fmt::Display for LyricsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LyricsError::NotFound => write!(f, "no lyrics found"),
            LyricsError::Message(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for LyricsError {}

#[derive(Debug, Deserialize)]
struct LrcLibItem {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NeteaseSearchResponse {
    result: Option<NeteaseSearchResult>,
}

#[derive(Debug, Deserialize)]
struct NeteaseSearchResult {
    songs: Option<Vec<NeteaseSong>>,
}

#[derive(Debug, Deserialize)]
struct NeteaseSong {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct NeteaseLyricResponse {
    lrc: Option<NeteaseLyricBlock>,
}

#[derive(Debug, Deserialize)]
struct NeteaseLyricBlock {
    lyric: Option<String>,
}

pub fn fetch(artist: &str, title: &str) -> Result<Vec<LyricLine>, LyricsError> {
    let mut artist = artist.trim().to_string();
    let mut title = title.trim().to_string();
    if artist.is_empty() && title.is_empty() {
        return Err(LyricsError::NotFound);
    }

    if let Some((a, t)) = title.split_once(" - ") {
        let a = clean_query(a.trim());
        let t = clean_query(t.trim());
        if !a.is_empty() && !t.is_empty() {
            artist = a;
            title = t;
        }
    }

    let query = format!("{} {}", clean_query(&artist), clean_query(&title))
        .trim()
        .to_string();
    let query = if query.is_empty() {
        format!("{artist} {title}").trim().to_string()
    } else {
        query
    };
    if query.is_empty() {
        return Err(LyricsError::NotFound);
    }

    let lrclib_err = match fetch_lrclib(&query) {
        Ok(lines) if !lines.is_empty() => return Ok(lines),
        Ok(_) => LyricsError::NotFound,
        Err(err) => err,
    };

    let netease_err = match fetch_netease(&query) {
        Ok(lines) if !lines.is_empty() => return Ok(lines),
        Ok(_) => LyricsError::NotFound,
        Err(err) => err,
    };

    if matches!(lrclib_err, LyricsError::NotFound) && matches!(netease_err, LyricsError::NotFound) {
        Err(LyricsError::NotFound)
    } else {
        Err(LyricsError::Message(format!(
            "lrclib: {lrclib_err}; netease: {netease_err}"
        )))
    }
}

fn fetch_lrclib(query: &str) -> Result<Vec<LyricLine>, LyricsError> {
    let url = format!(
        "https://lrclib.net/api/search?q={}",
        urlencoding::encode(query)
    );
    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .set("User-Agent", "rliamp")
        .call()
        .map_err(|err| LyricsError::Message(format!("request failed: {err}")))?;
    if resp.status() != 200 {
        return Err(LyricsError::Message(format!("http {}", resp.status())));
    }

    let body = read_body_limited(resp)?;
    let items: Vec<LrcLibItem> =
        serde_json::from_str(&body).map_err(|err| LyricsError::Message(err.to_string()))?;
    if items.is_empty() {
        return Err(LyricsError::NotFound);
    }

    for item in &items {
        if let Some(synced) = item.synced_lyrics.as_ref() {
            let parsed = parse_lrc(synced);
            if !parsed.is_empty() {
                return Ok(parsed);
            }
        }
    }

    if let Some(plain) = items.first().and_then(|it| it.plain_lyrics.as_ref()) {
        let mut lines = Vec::new();
        for raw in plain.lines() {
            let text = raw.trim();
            if !text.is_empty() {
                lines.push(LyricLine {
                    start: Duration::ZERO,
                    text: text.to_string(),
                });
            }
        }
        if !lines.is_empty() {
            return Ok(lines);
        }
    }

    Err(LyricsError::NotFound)
}

fn fetch_netease(query: &str) -> Result<Vec<LyricLine>, LyricsError> {
    let payload = format!("s={}&type=1&limit=1", urlencoding::encode(query));
    let resp = ureq::post("http://music.163.com/api/search/get/web")
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .set("Content-Type", "application/x-www-form-urlencoded")
        .set("Referer", "http://music.163.com")
        .send_string(&payload)
        .map_err(|err| LyricsError::Message(format!("search failed: {err}")))?;
    if resp.status() != 200 {
        return Err(LyricsError::Message(format!(
            "search http {}",
            resp.status()
        )));
    }

    let body = read_body_limited(resp)?;
    let search: NeteaseSearchResponse =
        serde_json::from_str(&body).map_err(|err| LyricsError::Message(err.to_string()))?;
    let song_id = search
        .result
        .and_then(|r| r.songs)
        .and_then(|mut songs| songs.drain(..).next())
        .map(|song| song.id)
        .ok_or(LyricsError::NotFound)?;

    let lyric_url = format!("http://music.163.com/api/song/lyric?id={song_id}&lv=1&kv=1&tv=-1");
    let lyric_resp = ureq::get(&lyric_url)
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .call()
        .map_err(|err| LyricsError::Message(format!("lyric fetch failed: {err}")))?;
    if lyric_resp.status() != 200 {
        return Err(LyricsError::Message(format!(
            "lyric http {}",
            lyric_resp.status()
        )));
    }

    let lyric_body = read_body_limited(lyric_resp)?;
    let lyric: NeteaseLyricResponse =
        serde_json::from_str(&lyric_body).map_err(|err| LyricsError::Message(err.to_string()))?;
    let lrc = lyric
        .lrc
        .and_then(|x| x.lyric)
        .ok_or(LyricsError::NotFound)?;

    let parsed = parse_lrc(&lrc);
    if parsed.is_empty() {
        return Err(LyricsError::NotFound);
    }
    Ok(parsed)
}

fn read_body_limited(response: ureq::Response) -> Result<String, LyricsError> {
    let mut body = response
        .into_string()
        .map_err(|err| LyricsError::Message(format!("read body failed: {err}")))?;
    if body.len() > MAX_RESPONSE_BODY {
        body.truncate(MAX_RESPONSE_BODY);
    }
    Ok(body)
}

fn clean_query(raw: &str) -> String {
    let stripped = strip_bracketed(raw).trim().to_string();
    if stripped.is_empty() {
        return stripped;
    }
    let lowered = stripped.to_lowercase();
    for marker in [" official", " lyric", " audio", " video"] {
        if let Some(pos) = lowered.find(marker) {
            return stripped[..pos]
                .trim_end_matches([' ', '-', '–', '—'])
                .trim()
                .to_string();
        }
    }
    stripped
}

fn strip_bracketed(raw: &str) -> String {
    let mut out = String::new();
    let mut paren_depth = 0usize;
    let mut square_depth = 0usize;
    for ch in raw.chars() {
        match ch {
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => square_depth = square_depth.saturating_add(1),
            ']' => square_depth = square_depth.saturating_sub(1),
            _ if paren_depth == 0 && square_depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn parse_lrc(raw: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (stamps, text) = parse_lrc_line(trimmed);
        if stamps.is_empty() {
            continue;
        }
        let text = text.trim();
        for stamp in stamps {
            out.push(LyricLine {
                start: stamp,
                text: text.to_string(),
            });
        }
    }

    out.sort_by_key(|line| line.start);
    out
}

fn parse_lrc_line(line: &str) -> (Vec<Duration>, &str) {
    let mut rest = line;
    let mut stamps = Vec::new();

    while let Some(after_l) = rest.strip_prefix('[') {
        let Some(end) = after_l.find(']') else {
            break;
        };
        let tag = &after_l[..end];
        if let Some(ts) = parse_timestamp(tag) {
            stamps.push(ts);
            rest = &after_l[end + 1..];
        } else {
            break;
        }
    }

    (stamps, rest)
}

fn parse_timestamp(tag: &str) -> Option<Duration> {
    let (mm, sec_part) = tag.split_once(':')?;
    let minutes = mm.trim().parse::<u64>().ok()?;

    let (seconds, millis) = if let Some((ss, frac)) = sec_part.split_once('.') {
        let seconds = ss.trim().parse::<u64>().ok()?;
        let frac = frac.trim();
        let millis = match frac.len() {
            0 => 0,
            1 => frac.parse::<u64>().ok()? * 100,
            2 => frac.parse::<u64>().ok()? * 10,
            _ => frac[..3.min(frac.len())].parse::<u64>().ok()?,
        };
        (seconds, millis)
    } else {
        (sec_part.trim().parse::<u64>().ok()?, 0)
    };

    Some(Duration::from_millis(
        minutes * 60_000 + seconds * 1000 + millis,
    ))
}
