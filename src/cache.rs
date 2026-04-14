use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::api::{self, Library};

const CACHE_FILE: &str = "libraries.json";
const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

fn cache_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("dev", "rnd", "rnd")
        .context("could not resolve cache dir")?;
    fs::create_dir_all(dirs.cache_dir())?;
    Ok(dirs.cache_dir().join(CACHE_FILE))
}

fn is_fresh(path: &PathBuf, ttl: Duration) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age < ttl)
        .unwrap_or(false)
}

pub async fn load(force_refresh: bool) -> Result<Vec<Library>> {
    let path = cache_path()?;

    if !force_refresh && is_fresh(&path, DEFAULT_TTL) {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(libs) = serde_json::from_slice::<Vec<Library>>(&bytes) {
                return Ok(libs);
            }
        }
    }

    let libs = api::fetch_all().await?;
    let bytes = serde_json::to_vec(&libs)?;
    fs::write(&path, bytes).context("writing cache")?;
    Ok(libs)
}

pub fn clear() -> Result<()> {
    let path = cache_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn location() -> Result<PathBuf> {
    cache_path()
}
