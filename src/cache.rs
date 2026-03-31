use crate::scanner::Manifest;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const CACHE_FILE: &str = ".uc-cache.json";

/// Persistent cache: maps file path → SHA-256 hex digest.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BuildCache {
    pub file_hashes: HashMap<String, String>,
}

/// Load the cache from disk (returns empty cache if not found).
pub fn load(root: &Path) -> Result<BuildCache> {
    let cache_path = root.join(CACHE_FILE);
    if !cache_path.exists() {
        return Ok(BuildCache::default());
    }
    let content = std::fs::read_to_string(&cache_path)?;
    let cache: BuildCache = serde_json::from_str(&content)?;
    Ok(cache)
}

/// Persist the current manifest's hashes to disk.
pub fn save(root: &Path, manifest: &Manifest) -> Result<()> {
    let mut file_hashes = HashMap::new();

    for path in &manifest.source_files {
        if let Some(hash) = hash_file(path) {
            let key = path.display().to_string();
            file_hashes.insert(key, hash);
        }
    }

    let cache = BuildCache { file_hashes };
    let content = serde_json::to_string_pretty(&cache)?;
    std::fs::write(root.join(CACHE_FILE), content)?;
    Ok(())
}

/// Remove the cache file.
pub fn clear(root: &Path) -> Result<()> {
    let cache_path = root.join(CACHE_FILE);
    if cache_path.exists() {
        std::fs::remove_file(cache_path)?;
    }
    Ok(())
}

/// Return the list of source files that have changed since the last build.
pub fn changed_files<'a>(manifest: &'a Manifest, cache: &BuildCache) -> Vec<&'a PathBuf> {
    manifest
        .source_files
        .iter()
        .filter(|path| {
            let key = path.display().to_string();
            match (cache.file_hashes.get(&key), hash_file(path)) {
                (Some(cached), Some(current)) => cached != &current,
                (None, _) => true,   // new file
                (_, None) => true,   // can't read — assume changed
            }
        })
        .collect()
}

fn hash_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Some(hex::encode(hasher.finalize()))
}
