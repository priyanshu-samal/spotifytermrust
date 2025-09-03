use crate::auth::Tokens;
use anyhow::Result;
use directories::ProjectDirs;
use serde_json;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
};

fn get_config_path() -> Option<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from("com", "spotify-tui-rs", "spotify-tui-rs") {
        let config_dir = proj_dirs.config_dir();
        if !config_dir.exists() {
            fs::create_dir_all(config_dir).ok()?;
        }
        Some(config_dir.join("spotify-tui-rs.json"))
    } else {
        None
    }
}

pub fn save_tokens(tokens: &Tokens) -> Result<()> {
    if let Some(config_path) = get_config_path() {
        let mut file = File::create(config_path)?;
        let json = serde_json::to_string(tokens)?;
        file.write_all(json.as_bytes())?;
    }
    Ok(())
}

pub fn load_tokens() -> Result<Option<Tokens>> {
    if let Some(config_path) = get_config_path() {
        if config_path.exists() {
            let mut file = File::open(config_path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            let tokens: Tokens = serde_json::from_str(&contents)?;
            return Ok(Some(tokens));
        }
    }
    Ok(None)
}