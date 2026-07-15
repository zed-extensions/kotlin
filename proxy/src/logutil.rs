use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cache::cache_root;

pub fn debug_enabled() -> bool {
    env_flag("KOTLIN_LSP_PROXY_DEBUG")
}

pub fn env_flag(name: &str) -> bool {
    matches!(
        env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn log_path() -> PathBuf {
    if let Ok(p) = env::var("KOTLIN_LSP_PROXY_LOG") {
        return PathBuf::from(p);
    }
    cache_root().join("proxy.log")
}

/// Log to stderr (Zed surfaces as language-server stderr) and append to the proxy log file.
pub fn log_line(msg: &str) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[kotlin-lsp-proxy {ts}] {msg}");
    eprintln!("{line}");
    if let Some(parent) = log_path().parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(f, "{line}");
    }
}
