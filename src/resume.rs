use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;
#[cfg(unix)]
use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeState {
    pub path: String,
    pub secs: u64,
}

pub fn load() -> io::Result<Option<ResumeState>> {
    let path = resume_path()?;
    let Ok(raw) = fs::read_to_string(path) else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<ResumeState>(&raw)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if parsed.path.trim().is_empty() || parsed.secs == 0 {
        return Ok(None);
    }
    Ok(Some(parsed))
}

pub fn save(state: &ResumeState) -> io::Result<()> {
    let path = resume_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string(state)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    #[cfg(unix)]
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(body.as_bytes())?;
        return file.flush();
    }

    #[cfg(not(unix))]
    {
        fs::write(path, body)
    }
}

pub fn clear() -> io::Result<()> {
    let path = resume_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn resume_path() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME not set"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("rliamp")
        .join("resume.json"))
}
