use crate::playlist::{is_feed, is_m3u, is_pls, is_url, is_xiaoyuzhou_episode, is_ytdl, Track};
use crate::ytdlp;
use anyhow::{anyhow, Context, Result};
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteInputKind {
    Feed,
    M3u,
    Pls,
    Other,
}

pub fn resolve_runtime_url(url: &str) -> Result<Vec<Track>> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(Vec::new());
    }
    if !is_url(url) {
        return Err(anyhow!("URL must start with http:// or https://"));
    }

    if is_xiaoyuzhou_episode(url) {
        return resolve_xiaoyuzhou_episode(url)
            .with_context(|| format!("resolving xiaoyuzhou episode: {url}"));
    }

    if is_ytdl(url) {
        return ytdlp::resolve_collection(url)
            .with_context(|| format!("resolving yt-dlp collection: {url}"));
    }

    match classify_remote_input(url).with_context(|| format!("sniffing remote input: {url}"))? {
        RemoteInputKind::Feed => {
            resolve_feed(url).with_context(|| format!("resolving feed: {url}"))
        }
        RemoteInputKind::M3u => resolve_m3u(url).with_context(|| format!("resolving m3u: {url}")),
        RemoteInputKind::Pls => resolve_pls(url).with_context(|| format!("resolving pls: {url}")),
        RemoteInputKind::Other => Ok(vec![Track::from_path(url.to_string())]),
    }
}

fn classify_remote_input(url: &str) -> Result<RemoteInputKind> {
    if is_feed(url) {
        return Ok(RemoteInputKind::Feed);
    }
    if is_m3u(url) {
        return Ok(RemoteInputKind::M3u);
    }
    if is_pls(url) {
        return Ok(RemoteInputKind::Pls);
    }
    sniff_remote_input_kind(url)
}

fn sniff_remote_input_kind(url: &str) -> Result<RemoteInputKind> {
    let response = ureq::get(url)
        .timeout(Duration::from_secs(10))
        .set("Range", "bytes=0-4095")
        .call()
        .map_err(|err| anyhow!("remote probe request failed: {err}"))?;

    if let Some(content_type) = response.header("Content-Type") {
        let ct = content_type.to_ascii_lowercase();
        if ct.contains("mpegurl") || ct.contains("x-mpegurl") {
            return Ok(RemoteInputKind::M3u);
        }
        if ct.contains("scpls") || ct.contains("x-scpls") {
            return Ok(RemoteInputKind::Pls);
        }
        if ct.contains("xml") || ct.contains("rss") || ct.contains("atom") {
            return Ok(RemoteInputKind::Feed);
        }
    }

    let mut limited = response.into_reader().take(4096);
    let mut head = String::new();
    limited
        .read_to_string(&mut head)
        .map_err(|err| anyhow!("failed reading remote probe body: {err}"))?;

    let body = head.trim_start().to_ascii_lowercase();
    if body.starts_with("#extm3u") || body.contains("#extinf:") {
        return Ok(RemoteInputKind::M3u);
    }
    if body.contains("[playlist]") && body.contains("file1=") {
        return Ok(RemoteInputKind::Pls);
    }
    if body.starts_with("<?xml") || body.contains("<rss") || body.contains("<feed") {
        return Ok(RemoteInputKind::Feed);
    }

    Ok(RemoteInputKind::Other)
}

#[derive(Debug, Deserialize, Default)]
struct XiaoyuzhouSchema {
    name: Option<String>,
    #[serde(rename = "associatedMedia")]
    associated_media: Option<XiaoyuzhouAssociatedMedia>,
    #[serde(rename = "partOfSeries")]
    part_of_series: Option<XiaoyuzhouPartOfSeries>,
}

#[derive(Debug, Deserialize, Default)]
struct XiaoyuzhouAssociatedMedia {
    #[serde(rename = "contentUrl")]
    content_url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct XiaoyuzhouPartOfSeries {
    name: Option<String>,
}

fn resolve_xiaoyuzhou_episode(page_url: &str) -> Result<Vec<Track>> {
    let response = ureq::get(page_url)
        .timeout(Duration::from_secs(15))
        .set(
            "User-Agent",
            "rliamp/0.1 (+https://github.com/0smboy/rliamp)",
        )
        .call()
        .map_err(|err| anyhow!("request failed: {err}"))?;

    let mut body = String::new();
    response
        .into_reader()
        .take(2 << 20)
        .read_to_string(&mut body)
        .map_err(|err| anyhow!("failed to read xiaoyuzhou page: {err}"))?;

    Ok(vec![parse_xiaoyuzhou_episode_html(page_url, &body)?])
}

fn parse_xiaoyuzhou_episode_html(page_url: &str, doc: &str) -> Result<Track> {
    let mut audio_url = extract_meta_content(doc, "property", "og:audio");
    let mut title = extract_meta_content(doc, "property", "og:title");
    let mut artist = None;

    if let Some(raw_schema) = extract_xiaoyuzhou_schema_json(doc) {
        let schema: XiaoyuzhouSchema = serde_json::from_str(&raw_schema)
            .with_context(|| "parsing xiaoyuzhou schema.org JSON-LD")?;
        if audio_url.is_none() {
            audio_url = schema.associated_media.and_then(|media| media.content_url);
        }
        if title.is_none() {
            title = schema.name;
        }
        if artist.is_none() {
            artist = schema.part_of_series.and_then(|series| series.name);
        }
    }

    let path = audio_url.ok_or_else(|| anyhow!("audio URL not found in xiaoyuzhou page"))?;
    let title = title.unwrap_or_else(|| page_url.to_string());

    Ok(Track {
        path,
        title,
        artist: artist.unwrap_or_default(),
        stream: true,
        ytdlp: false,
    })
}

fn extract_xiaoyuzhou_schema_json(doc: &str) -> Option<String> {
    let lower = doc.to_ascii_lowercase();
    let idx = lower
        .find("name=\"schema:podcast-show\"")
        .or_else(|| lower.find("name='schema:podcast-show'"))?;
    let after = &doc[idx..];
    let open = after.find('>')?;
    let rest = &after[open + 1..];
    let close = rest.to_ascii_lowercase().find("</script>")?;
    let raw = rest[..close].trim();
    if raw.is_empty() {
        None
    } else {
        Some(html_unescape(raw))
    }
}

fn extract_meta_content(doc: &str, attr: &str, value: &str) -> Option<String> {
    let mut rest = doc;
    loop {
        let lower = rest.to_ascii_lowercase();
        let start = lower.find("<meta")?;
        let tail = &rest[start..];
        let end = tail.find('>')?;
        let tag = &tail[..=end];
        if extract_attr(tag, attr)
            .map(|found| found.eq_ignore_ascii_case(value))
            .unwrap_or(false)
        {
            if let Some(content) = extract_attr(tag, "content") {
                let content = html_unescape(content.trim());
                if !content.is_empty() {
                    return Some(content);
                }
            }
        }
        rest = &tail[end + 1..];
    }
}

fn extract_attr<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{attr}=");
    let idx = lower.find(&needle)?;
    let start = idx + needle.len();
    let bytes = tag.as_bytes();
    let quote = *bytes.get(start)?;

    if quote == b'"' || quote == b'\'' {
        let value_start = start + 1;
        let end_rel = tag[value_start..].find(quote as char)?;
        return Some(&tag[value_start..value_start + end_rel]);
    }

    let tail = &tag[start..];
    let end = tail
        .find(|ch: char| ch.is_ascii_whitespace() || ch == '>' || ch == '/')
        .unwrap_or(tail.len());
    Some(&tail[..end])
}

fn html_unescape(raw: &str) -> String {
    unescape(raw)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| raw.to_string())
}

fn resolve_m3u(url: &str) -> Result<Vec<Track>> {
    let response = ureq::get(url)
        .call()
        .map_err(|err| anyhow!("request failed: {err}"))?;
    let body = response
        .into_string()
        .map_err(|err| anyhow!("failed to read m3u body: {err}"))?;
    Ok(parse_m3u_tracks(&body))
}

fn parse_m3u_tracks(body: &str) -> Vec<Track> {
    let mut tracks = Vec::new();
    let mut pending_title: Option<String> = None;

    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let title = rest
                .split_once(',')
                .map(|(_, t)| t.trim())
                .unwrap_or_default()
                .to_string();
            if !title.is_empty() {
                pending_title = Some(title);
            }
            continue;
        }

        if line.starts_with('#') || !is_url(line) {
            continue;
        }

        let mut track = Track::from_path(line.to_string());
        if let Some(title) = pending_title.take() {
            apply_title_hint(&mut track, title);
        }
        tracks.push(track);
    }

    tracks
}

fn resolve_pls(url: &str) -> Result<Vec<Track>> {
    let response = ureq::get(url)
        .call()
        .map_err(|err| anyhow!("request failed: {err}"))?;
    let body = response
        .into_string()
        .map_err(|err| anyhow!("failed to read pls body: {err}"))?;
    parse_pls_tracks(&body)
}

fn parse_pls_tracks(body: &str) -> Result<Vec<Track>> {
    let mut files = BTreeMap::<usize, String>::new();
    let mut titles = BTreeMap::<usize, String>::new();

    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('[')
            || line.starts_with(';')
            || line.starts_with('#')
        {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        let lower = key.to_ascii_lowercase();
        if let Some(num) = lower
            .strip_prefix("file")
            .and_then(|s| s.parse::<usize>().ok())
        {
            files.insert(num, value.to_string());
            continue;
        }
        if let Some(num) = lower
            .strip_prefix("title")
            .and_then(|s| s.parse::<usize>().ok())
        {
            titles.insert(num, value.to_string());
        }
    }

    if files.is_empty() {
        return Err(anyhow!("no entries found in PLS playlist"));
    }

    let all_streams = files.len() > 1 && files.values().all(|p| is_url(p));
    if all_streams {
        let (&first_idx, first_path) = files
            .iter()
            .next()
            .ok_or_else(|| anyhow!("no entries found in PLS playlist"))?;
        let mut track = Track::from_path(first_path.to_string());
        if let Some(title) = titles.get(&first_idx) {
            let cleaned = strip_mirror_suffix(title.trim());
            if !cleaned.is_empty() {
                apply_title_hint(&mut track, cleaned.to_string());
            }
        }
        return Ok(vec![track]);
    }

    let mut out = Vec::new();
    for (idx, raw_path) in files {
        if !is_url(&raw_path) {
            continue;
        }
        let mut track = Track::from_path(raw_path);
        if let Some(title) = titles.get(&idx) {
            apply_title_hint(&mut track, title.trim().to_string());
        }
        out.push(track);
    }
    Ok(out)
}

fn apply_title_hint(track: &mut Track, title: String) {
    if let Some((artist, song)) = title.split_once(" - ") {
        if track.artist.is_empty() {
            track.artist = artist.trim().to_string();
        }
        track.title = song.trim().to_string();
    } else if !title.trim().is_empty() {
        track.title = title.trim().to_string();
    }
}

fn strip_mirror_suffix(s: &str) -> &str {
    if let Some(i) = s.rfind("(#") {
        if s.ends_with(')') {
            return s[..i].trim_end_matches([' ', ':']).trim();
        }
    }
    s
}

fn resolve_feed(url: &str) -> Result<Vec<Track>> {
    let response = ureq::get(url)
        .call()
        .map_err(|err| anyhow!("request failed: {err}"))?;
    let xml = response
        .into_string()
        .map_err(|err| anyhow!("failed to read feed body: {err}"))?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut in_item = false;
    let mut current_tag: Vec<u8> = Vec::new();

    let mut channel_title = String::new();
    let mut item_title = String::new();
    let mut enclosure_url = String::new();
    let mut tracks = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_vec();
                if name.as_slice() == b"item" {
                    in_item = true;
                    item_title.clear();
                    enclosure_url.clear();
                }
                current_tag = name;
            }
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"enclosure" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"url" {
                            enclosure_url =
                                String::from_utf8_lossy(attr.value.as_ref()).to_string();
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let raw = String::from_utf8_lossy(t.as_ref());
                let text = quick_xml::escape::unescape(&raw)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| raw.into_owned());

                if current_tag.as_slice() == b"title" {
                    if in_item {
                        item_title = text;
                    } else if channel_title.is_empty() {
                        channel_title = text;
                    }
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"item" {
                    if !enclosure_url.is_empty() {
                        tracks.push(Track {
                            path: enclosure_url.clone(),
                            title: if item_title.is_empty() {
                                "Untitled Episode".to_string()
                            } else {
                                item_title.clone()
                            },
                            artist: channel_title.clone(),
                            stream: true,
                            ytdlp: false,
                        });
                    }
                    in_item = false;
                    item_title.clear();
                    enclosure_url.clear();
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(anyhow!("xml parse error: {err}")),
            _ => {}
        }
        buf.clear();
    }

    Ok(tracks)
}
