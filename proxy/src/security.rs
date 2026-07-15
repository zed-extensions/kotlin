//! Path safety for archive opens and cache writes (zip-slip, allowlists).

use std::env;
use std::path::{Component, Path, PathBuf};

use crate::logutil::{env_flag, log_line};

/// Reject zip entries that would escape the cache root (zip-slip).
pub fn safe_relative_entry(entry: &str) -> Option<PathBuf> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    // Jar members sometimes start with a single `/`; strip at most one.
    // Do not strip all slashes — that would turn absolute paths into relative ones.
    let entry = entry.strip_prefix('/').unwrap_or(entry);
    if entry.is_empty() || entry.starts_with('/') || entry.starts_with('\\') {
        return None;
    }
    let path = Path::new(entry);
    if path.is_absolute() {
        return None;
    }
    for c in path.components() {
        match c {
            Component::Normal(_) => {}
            Component::CurDir => {}
            // Zip-slip / volume roots
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(path.to_path_buf())
}

/// Whether reading this archive path is allowed.
///
/// Default: allow common dependency/JDK locations under the user home and
/// well-known JVM install roots. Set `KOTLIN_LSP_PROXY_ALLOW_ANY_ARCHIVE=1`
/// to allow any readable regular file (power users / unusual layouts).
pub fn archive_path_allowed(path: &Path) -> bool {
    if env_flag("KOTLIN_LSP_PROXY_ALLOW_ANY_ARCHIVE") {
        return path.is_file();
    }

    let Ok(canon) = path.canonicalize() else {
        // Fall back to lexical checks if canonicalize fails (missing file handled elsewhere)
        return lexical_allowed(path);
    };

    if !canon.is_file() {
        return false;
    }

    lexical_allowed(&canon)
}

fn lexical_allowed(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let lower = s.to_ascii_lowercase();

    // Name heuristics: dependency sources / JDK sources.
    let name_ok = lower.ends_with("-sources.jar")
        || lower.ends_with("sources.jar")
        || lower.ends_with("src.zip")
        || lower.ends_with("-src.zip")
        || lower.ends_with(".jar")
        || lower.ends_with(".zip");

    if !name_ok {
        return false;
    }

    if let Some(home) = env::var_os("HOME") {
        if path.starts_with(Path::new(&home)) {
            return true;
        }
    }
    if let Some(userprofile) = env::var_os("USERPROFILE") {
        if path.starts_with(Path::new(&userprofile)) {
            return true;
        }
    }
    if let Some(java_home) = env::var_os("JAVA_HOME") {
        if path.starts_with(Path::new(&java_home)) {
            return true;
        }
    }

    // Common system JDK locations (macOS / Linux).
    const PREFIXES: &[&str] = &[
        "/Library/Java/",
        "/opt/homebrew/",
        "/usr/lib/jvm/",
        "/usr/lib64/jvm/",
        "/usr/local/java/",
        "/opt/java/",
        "/opt/jdk",
    ];
    for p in PREFIXES {
        if s.starts_with(p) || lower.starts_with(&p.to_ascii_lowercase()) {
            return true;
        }
    }

    // Windows-style Program Files JDKs
    if (lower.contains("\\java\\") || lower.contains("/java/"))
        && (lower.contains("program files") || lower.contains("\\jdk") || lower.contains("/jdk"))
    {
        return true;
    }

    log_line(&format!(
        "archive path not on allowlist (set KOTLIN_LSP_PROXY_ALLOW_ANY_ARCHIVE=1 to override): {s}"
    ));
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zip_slip() {
        assert!(safe_relative_entry("../etc/passwd").is_none());
        assert!(safe_relative_entry("a/../../b").is_none());
        assert!(safe_relative_entry("//abs/path").is_none());
        // A single leading slash is normal for some jar members and is stripped.
        assert!(safe_relative_entry("/org/foo/Bar.java").is_some());
    }

    #[test]
    fn accepts_normal_entry() {
        assert_eq!(
            safe_relative_entry("org/foo/Bar.java").unwrap(),
            PathBuf::from("org/foo/Bar.java")
        );
        assert_eq!(
            safe_relative_entry("/org/foo/Bar.java").unwrap(),
            PathBuf::from("org/foo/Bar.java")
        );
    }
}
