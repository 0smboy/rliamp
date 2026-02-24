mod player;
mod playlist;
mod ui;
mod visualizer;

use anyhow::{anyhow, Result};
use glob::glob;
use playlist::{Playlist, Track};
use std::env;
use std::path::PathBuf;
use std::process;

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return Err(anyhow!("usage: rliamp <file.mp3> [file2.mp3 ...]"));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for arg in args {
        match glob(&arg) {
            Ok(paths) => {
                let mut matched = false;
                for entry in paths.flatten() {
                    files.push(entry);
                    matched = true;
                }
                if !matched {
                    files.push(PathBuf::from(arg));
                }
            }
            Err(_) => files.push(PathBuf::from(arg)),
        }
    }

    let mut playlist = Playlist::new();
    playlist.add(
        files
            .into_iter()
            .map(|path| Track::from_path(path.to_string_lossy().to_string())),
    );

    let player = player::Player::new()?;
    let mut app = ui::App::new(player, playlist);
    app.run()
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}
