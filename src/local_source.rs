use crate::player;
use crate::playlist::{is_url, Track};
use anyhow::{anyhow, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn resolve_local_input(
    path: &Path,
    files: &mut Vec<PathBuf>,
    tracks: &mut Vec<Track>,
) -> Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };

    if meta.is_file() {
        if is_local_m3u(path) {
            tracks.extend(resolve_local_m3u(path)?);
            return Ok(());
        }
        if is_local_pls(path) {
            tracks.extend(resolve_local_pls(path)?);
            return Ok(());
        }
        if player::is_supported_path(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if meta.is_dir() {
        collect_audio_files(path, files)?;
    }

    Ok(())
}

pub fn resolve_paths_to_tracks(paths: &[PathBuf]) -> Result<Vec<Track>> {
    let mut files = Vec::new();
    let mut tracks = Vec::new();
    for path in paths {
        resolve_local_input(path, &mut files, &mut tracks)?;
    }

    files.sort();
    files.dedup();
    let mut out = files
        .into_iter()
        .map(|path| Track::from_path(path.to_string_lossy().to_string()))
        .collect::<Vec<_>>();
    out.extend(tracks);
    Ok(out)
}

pub fn is_browser_selectable_path(path: &Path) -> bool {
    player::is_supported_path(path) || is_local_m3u(path) || is_local_pls(path)
}

fn is_local_m3u(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(ext.to_ascii_lowercase().as_str(), "m3u" | "m3u8")
}

fn is_local_pls(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    ext.eq_ignore_ascii_case("pls")
}

fn collect_audio_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };

    if meta.is_file() {
        if player::is_supported_path(path) {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }

    if !meta.is_dir() {
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(path)?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .collect();
    entries.sort();

    for entry in entries {
        collect_audio_files(&entry, out)?;
    }

    Ok(())
}

fn resolve_local_m3u(path: &Path) -> Result<Vec<Track>> {
    let body =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(parse_m3u_tracks(&body, path.parent()))
}

fn parse_m3u_tracks(body: &str, base_dir: Option<&Path>) -> Vec<Track> {
    let mut tracks = Vec::new();
    let mut pending_title: Option<String> = None;
    let mut pending_duration_secs: Option<u32> = None;
    let mut pending_realtime = false;

    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (duration_secs, realtime, title) = parse_extinf(rest);
            pending_duration_secs = duration_secs;
            pending_realtime = realtime;
            if !title.is_empty() {
                pending_title = Some(title);
            }
            continue;
        }

        if line.starts_with('#') {
            continue;
        }

        let mut track = if is_url(line) {
            Track::from_path(line.to_string())
        } else {
            if base_dir.is_none() {
                continue;
            }

            let Some(resolved) = resolve_local_playlist_path(base_dir, line) else {
                continue;
            };
            Track::from_path(resolved.to_string_lossy().to_string())
        };

        if let Some(title) = pending_title.take() {
            apply_title_hint(&mut track, title);
        }
        if let Some(duration_secs) = pending_duration_secs.take() {
            track.duration_secs = duration_secs;
        }
        track.realtime = pending_realtime;
        pending_realtime = false;
        tracks.push(track);
    }

    tracks
}

fn resolve_local_pls(path: &Path) -> Result<Vec<Track>> {
    let body =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_pls_tracks(&body, path.parent())
}

fn parse_pls_tracks(body: &str, base_dir: Option<&Path>) -> Result<Vec<Track>> {
    let mut files = BTreeMap::<usize, String>::new();
    let mut titles = BTreeMap::<usize, String>::new();
    let mut lengths = BTreeMap::<usize, i32>::new();

    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('[')
            || line.starts_with(';')
            || line.starts_with('#')
        {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let lower = key.to_ascii_lowercase();
        if let Some(num) = lower
            .strip_prefix("file")
            .and_then(|part| part.parse::<usize>().ok())
        {
            files.insert(num, value.to_string());
            continue;
        }
        if let Some(num) = lower
            .strip_prefix("title")
            .and_then(|part| part.parse::<usize>().ok())
        {
            titles.insert(num, value.to_string());
            continue;
        }
        if let Some(num) = lower
            .strip_prefix("length")
            .and_then(|part| part.parse::<usize>().ok())
        {
            if let Ok(length) = value.parse::<i32>() {
                lengths.insert(num, length);
            }
        }
    }

    if files.is_empty() {
        return Err(anyhow!("no entries found in PLS playlist"));
    }

    let all_streams = files.len() > 1 && files.values().all(|path| is_url(path));
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
        apply_pls_length(&mut track, lengths.get(&first_idx).copied(), true);
        return Ok(vec![track]);
    }

    let mut out = Vec::new();
    for (idx, raw_path) in files {
        let mut track = if is_url(&raw_path) {
            Track::from_path(raw_path)
        } else {
            if base_dir.is_none() {
                continue;
            }

            let Some(resolved) = resolve_local_playlist_path(base_dir, raw_path.as_str()) else {
                continue;
            };
            Track::from_path(resolved.to_string_lossy().to_string())
        };
        if let Some(title) = titles.get(&idx) {
            apply_title_hint(&mut track, title.trim().to_string());
        }
        apply_pls_length(&mut track, lengths.get(&idx).copied(), false);
        out.push(track);
    }
    Ok(out)
}

fn parse_extinf(rest: &str) -> (Option<u32>, bool, String) {
    let mut duration_secs = None;
    let mut realtime = false;
    let title = if let Some((dur, raw_title)) = rest.split_once(',') {
        if let Ok(parsed) = dur.trim().parse::<i32>() {
            if parsed < 0 {
                realtime = true;
            } else {
                duration_secs = Some(parsed as u32);
            }
        }
        raw_title.trim().to_string()
    } else {
        rest.trim().to_string()
    };

    (duration_secs, realtime, title)
}

fn apply_pls_length(track: &mut Track, length: Option<i32>, assume_realtime_stream: bool) {
    match length {
        Some(value) if value < 0 => {
            track.realtime = true;
            track.duration_secs = 0;
        }
        Some(value) if value > 0 => {
            track.duration_secs = value as u32;
        }
        _ if assume_realtime_stream && track.stream => {
            track.realtime = true;
        }
        _ => {}
    }
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
    if let Some(idx) = s.rfind("(#") {
        if s.ends_with(')') {
            return s[..idx].trim_end_matches([' ', ':']).trim();
        }
    }
    s
}

fn resolve_local_playlist_path(base_dir: Option<&Path>, raw: &str) -> Option<PathBuf> {
    if raw.is_empty() || raw.contains('\0') {
        return None;
    }

    let path = Path::new(raw);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }

    let Some(base) = base_dir else {
        return Some(path.to_path_buf());
    };
    let normalized_base = normalize_path_lexical(base);
    let normalized_target = normalize_path_lexical(base.join(path));

    if !normalized_target.starts_with(&normalized_base) {
        return None;
    }
    Some(normalized_target)
}

fn normalize_path_lexical(path: impl AsRef<Path>) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.as_ref().components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
