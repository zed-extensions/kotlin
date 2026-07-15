//! On-disk cache for extracted archive entries + light GC.

use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::logutil::log_line;

/// Default max cache size (~512 MiB). Override with `KOTLIN_LSP_PROXY_CACHE_MAX_MB`.
const DEFAULT_MAX_CACHE_MB: u64 = 512;

pub fn cache_root() -> PathBuf {
    if let Ok(dir) = env::var("KOTLIN_LSP_PROXY_CACHE") {
        return PathBuf::from(dir);
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("zed-kotlin-jar-sources");
    }
    // Windows / fallback
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("zed-kotlin-jar-sources");
    }
    env::temp_dir().join("zed-kotlin-jar-sources")
}

pub fn cache_file_for(archive: &Path, entry: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(archive.to_string_lossy().as_bytes());
    if let Ok(meta) = fs::metadata(archive) {
        if let Ok(modified) = meta.modified() {
            if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(dur.as_secs().to_le_bytes());
            }
        }
        hasher.update(meta.len().to_le_bytes());
    }
    hasher.update(entry.as_bytes());
    let digest = hex_encode(hasher.finalize());
    cache_root().join(&digest[..16]).join(entry)
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

fn max_cache_bytes() -> u64 {
    env::var("KOTLIN_LSP_PROXY_CACHE_MAX_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_CACHE_MB)
        .saturating_mul(1024 * 1024)
}

/// Best-effort: if the cache exceeds the configured size, delete oldest files first.
pub fn maybe_gc_cache() {
    let root = cache_root();
    if !root.is_dir() {
        return;
    }

    let max = max_cache_bytes();
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let mut total: u64 = 0;

    let walker = walkdir(&root);
    for path in walker {
        if let Ok(meta) = fs::metadata(&path) {
            if meta.is_file() {
                // Skip the log file
                if path.file_name().and_then(|n| n.to_str()) == Some("proxy.log") {
                    continue;
                }
                let len = meta.len();
                total = total.saturating_add(len);
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                files.push((path, len, modified));
            }
        }
    }

    if total <= max {
        return;
    }

    files.sort_by_key(|(_, _, m)| *m);
    let mut removed = 0u64;
    for (path, len, _) in files {
        if total <= max {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
            removed += 1;
        }
    }
    if removed > 0 {
        log_line(&format!(
            "cache GC removed {removed} file(s); ~{} MiB budget",
            max / (1024 * 1024)
        ));
    }
}

fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}
