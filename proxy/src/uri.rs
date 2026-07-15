//! Parse and materialize archive URIs into cached real files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cache::{cache_file_for, maybe_gc_cache};
use crate::logutil::log_line;
use crate::security::{archive_path_allowed, safe_relative_entry};

pub fn looks_like_archive_uri(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    (lower.contains(".jar") || lower.contains(".zip") || lower.starts_with("jar:"))
        && (s.contains('!') || lower.contains("%21"))
}

pub fn body_has_archive_uri(body: &[u8]) -> bool {
    contains(body, b".jar!")
        || contains(body, b".zip!")
        || contains(body, b".JAR!")
        || contains(body, b".ZIP!")
        || contains(body, b".jar%21")
        || contains(body, b".zip%21")
        || contains(body, b".jar%2521")
        || contains(body, b"jar:file:")
        || contains(body, b"jar://")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Turn an archive URI into a `file://` URI under the cache, or `None` to keep the original.
pub fn materialize_archive_uri(uri: &str) -> Option<String> {
    let (archive_path, entry) = split_archive_ref(uri)?;
    let entry = safe_relative_entry(&entry)?;
    let entry_str = entry.to_string_lossy();

    if !archive_path_allowed(&archive_path) {
        return None;
    }
    if !archive_path.is_file() {
        log_line(&format!(
            "archive missing: {} (uri={uri})",
            archive_path.display()
        ));
        return None;
    }

    let cache_file = cache_file_for(&archive_path, &entry_str);
    if !cache_file.is_file() {
        if let Err(e) = extract_entry(&archive_path, &entry_str, &cache_file) {
            log_line(&format!(
                "extract failed {}!{}: {e}",
                archive_path.display(),
                entry_str
            ));
            return None;
        }
        log_line(&format!(
            "extracted {}!{} -> {}",
            archive_path.display(),
            entry_str,
            cache_file.display()
        ));
        maybe_gc_cache();
    }

    match fs::metadata(&cache_file) {
        Ok(m) if m.is_file() && m.len() > 0 => {}
        Ok(m) => {
            log_line(&format!(
                "extract unusable (len={}): {}",
                m.len(),
                cache_file.display()
            ));
            let _ = fs::remove_file(&cache_file);
            return None;
        }
        Err(e) => {
            log_line(&format!("extract unreadable {}: {e}", cache_file.display()));
            return None;
        }
    }

    Some(path_to_file_uri(&cache_file))
}

pub fn split_archive_ref(uri: &str) -> Option<(PathBuf, String)> {
    let decoded = percent_decode(uri);

    let bang = decoded.find('!')?;
    let (left, right) = decoded.split_at(bang);
    let entry = right.trim_start_matches('!').trim_start_matches('/');
    if entry.is_empty() {
        return None;
    }

    let left_lower = left.to_ascii_lowercase();
    let looks_like_archive = left_lower.contains(".jar")
        || left_lower.contains(".zip")
        || left_lower.starts_with("jar:");
    if !looks_like_archive {
        return None;
    }

    let path = filesystem_path_from_uri_prefix(left)?;
    Some((path, entry.to_string()))
}

fn filesystem_path_from_uri_prefix(raw: &str) -> Option<PathBuf> {
    let mut s = raw.trim().to_string();

    if let Some(rest) = s.strip_prefix("jar:file://") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix("jar://") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix("file://") {
        s = rest.to_string();
    }

    if let Some(rest) = s.strip_prefix("localhost") {
        s = rest.to_string();
    }

    s = percent_decode(&s);

    if s.is_empty() {
        return None;
    }

    if s.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            s = PathBuf::from(home)
                .join(&s[2..])
                .to_string_lossy()
                .into_owned();
        }
    }

    #[cfg(windows)]
    {
        if s.starts_with('/') && s.len() >= 3 && s.as_bytes()[2] == b':' {
            s = s[1..].to_string();
        }
    }

    Some(PathBuf::from(s))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn extract_entry(archive: &Path, entry: &str, dest: &Path) -> io::Result<()> {
    // Destination must stay under cache root (defense in depth with safe_relative_entry).
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(io_err)?;

    let candidates = [
        entry.to_string(),
        entry.trim_start_matches('/').to_string(),
        format!("/{entry}"),
    ];

    let mut found = None;
    for name in &candidates {
        if zip.by_name(name).is_ok() {
            found = Some(name.clone());
            break;
        }
    }

    if found.is_none() {
        let target = entry.trim_start_matches('/').to_ascii_lowercase();
        for i in 0..zip.len() {
            let file = zip.by_index(i).map_err(io_err)?;
            // Also reject zip-slip inside the archive member name.
            if safe_relative_entry(file.name()).is_none() {
                continue;
            }
            let name = file.name().to_string();
            if name.trim_start_matches('/').to_ascii_lowercase() == target {
                found = Some(name);
                break;
            }
        }
    }

    let name = found.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("entry not found in archive: {entry}"),
        )
    })?;

    if safe_relative_entry(&name).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "refusing zip entry with path traversal",
        ));
    }

    let mut zf = zip.by_name(&name).map_err(io_err)?;
    let mut out = fs::File::create(dest)?;
    io::copy(&mut zf, &mut out)?;
    Ok(())
}

fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

fn path_to_file_uri(path: &Path) -> String {
    #[cfg(unix)]
    {
        format!("file://{}", path.display())
    }
    #[cfg(windows)]
    {
        let s = path.display().to_string().replace('\\', "/");
        format!("file:///{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_maven_sources_jar_path() {
        let uri = "/Users/x/.m2/repository/org/springframework/spring-web/7.0.8/spring-web-7.0.8-sources.jar!/org/springframework/web/bind/annotation/RequestParam.java";
        let (archive, entry) = split_archive_ref(uri).unwrap();
        assert!(archive.ends_with("spring-web-7.0.8-sources.jar"));
        assert_eq!(
            entry,
            "org/springframework/web/bind/annotation/RequestParam.java"
        );
    }

    #[test]
    fn splits_percent_encoded_bang() {
        let uri = "file:///Users/x/lib/foo-sources.jar%21/com/Example.java";
        let (archive, entry) = split_archive_ref(uri).unwrap();
        assert_eq!(archive, PathBuf::from("/Users/x/lib/foo-sources.jar"));
        assert_eq!(entry, "com/Example.java");
    }

    #[test]
    fn splits_file_uri_form() {
        let uri = "file:///Users/x/lib/foo-sources.jar!/com/Example.java";
        let (archive, entry) = split_archive_ref(uri).unwrap();
        assert_eq!(archive, PathBuf::from("/Users/x/lib/foo-sources.jar"));
        assert_eq!(entry, "com/Example.java");
    }

    #[test]
    fn splits_jetbrains_jar_triple_slash_uri() {
        let uri = "jar:///Users/x/.m2/repository/org/springframework/spring-web/7.0.8/spring-web-7.0.8-sources.jar!/org/springframework/web/bind/annotation/RequestParam.java";
        let (archive, entry) = split_archive_ref(uri).unwrap();
        assert_eq!(
            archive,
            PathBuf::from(
                "/Users/x/.m2/repository/org/springframework/spring-web/7.0.8/spring-web-7.0.8-sources.jar"
            )
        );
        assert_eq!(
            entry,
            "org/springframework/web/bind/annotation/RequestParam.java"
        );
    }

    #[test]
    fn materializes_jetbrains_jar_uri_if_present() {
        let home = std::env::var("HOME").unwrap();
        let jar = format!(
            "{home}/.m2/repository/org/springframework/spring-web/7.0.8/spring-web-7.0.8-sources.jar"
        );
        if !Path::new(&jar).is_file() {
            return;
        }
        let uri = format!("jar://{jar}!/org/springframework/web/bind/annotation/RequestParam.java");
        assert!(uri.starts_with("jar:///"), "got {uri}");
        let out = materialize_archive_uri(&uri).expect("materialize jetbrains uri");
        assert!(out.starts_with("file://"));
        let path = out.trim_start_matches("file://");
        let body = fs::read_to_string(path).expect("read");
        assert!(body.contains("interface RequestParam"));
    }

    #[test]
    fn materializes_spring_sources_if_present() {
        let home = std::env::var("HOME").unwrap();
        let jar = PathBuf::from(format!(
            "{home}/.m2/repository/org/springframework/spring-web/7.0.8/spring-web-7.0.8-sources.jar"
        ));
        if !jar.is_file() {
            return;
        }
        let uri = format!(
            "{}!/org/springframework/web/bind/annotation/RequestParam.java",
            jar.display()
        );
        let out = materialize_archive_uri(&uri).expect("materialize");
        assert!(out.starts_with("file://"));
        let path = out.trim_start_matches("file://");
        let body = fs::read_to_string(path).expect("read extracted");
        assert!(body.contains("interface RequestParam"));
    }
}
