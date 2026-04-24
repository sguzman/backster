use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn cache_dir() -> PathBuf {
    PathBuf::from(".cache").join("backster")
}

pub fn cache_path_for_key(key_hex: &str) -> PathBuf {
    cache_dir().join(format!("{key_hex}.json"))
}

pub fn read_cached_json(key_hex: &str) -> Result<Option<String>> {
    let path = cache_path_for_key(key_hex);
    if !path.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read cache file {}", path.display()))?;
    Ok(Some(s))
}

pub fn write_cached_json(key_hex: &str, contents: &str) -> Result<()> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create cache dir {}", dir.display()))?;
    let path = cache_path_for_key(key_hex);
    std::fs::write(&path, contents)
        .with_context(|| format!("Failed to write cache file {}", path.display()))?;
    Ok(())
}

pub fn blake3_hex(parts: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    for p in parts {
        hasher.update(p);
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

pub fn read_file_bytes(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("Failed to read {}", path.display()))
}

