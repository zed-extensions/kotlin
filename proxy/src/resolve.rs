//! Resolve archive-internal URIs returned by kotlin-lsp into real, readable files.
//!
//! kotlin-lsp points navigation targets at sources that live *inside* an archive,
//! e.g. `file:///home/u/.gradle/.../foo-sources.jar!/pkg/Foo.kt` or
//! `file:///.../lib/src.zip!/java.base/java/time/Instant.java`. The `!/` segment is
//! JetBrains' in-archive separator, so the path is not a real file on disk and Zed
//! opens an empty buffer.
//!
//! This module detects such URIs, extracts the entry from the `.jar`/`.zip` to a
//! stable temp file, and rewrites the location to a plain `file://` URI Zed can open.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::PathBuf;
use std::{env, fs};

use serde_json::Value;

/// Directory holding extracted archive sources.
fn cache_dir() -> PathBuf {
    env::temp_dir().join("kotlin-lsp-sources")
}

/// Rewrite archive (`…jar!/…`, `…zip!/…`) URIs in a `definition`/`typeDefinition`/
/// `implementation`/`declaration`/`references` response. The `result` may be a single
/// `Location`, an array of `Location`, or an array of `LocationLink` (which uses
/// `targetUri`). Returns `true` if any URI was rewritten.
pub fn rewrite_archive_locations(msg: &mut Value) -> bool {
    let results = match msg.get_mut("result") {
        Some(Value::Array(arr)) => arr.iter_mut().collect::<Vec<_>>(),
        Some(obj @ Value::Object(_)) => vec![obj],
        _ => return false,
    };

    let mut rewritten = false;
    for loc in results {
        for key in ["uri", "targetUri"] {
            let current = loc.get(key).and_then(|v| v.as_str()).map(str::to_owned);
            if let Some(uri) = current {
                if let Some(new_uri) = resolve_archive_uri(&uri) {
                    loc[key] = Value::String(new_uri);
                    rewritten = true;
                }
            }
        }
    }
    rewritten
}

/// If `uri` points inside a jar/zip archive, extract the entry to a temp file and
/// return a `file://` URI for it. Returns `None` when `uri` is not an archive URI or
/// extraction fails (caller then leaves the original URI untouched).
pub fn resolve_archive_uri(uri: &str) -> Option<String> {
    let (archive_uri, inner) = split_archive_uri(uri)?;
    let archive_path = uri_to_path(&archive_uri);
    let inner = percent_decode(inner.trim_start_matches('/'));

    let file_name = inner.rsplit('/').next().unwrap_or("Unknown");
    let (stem, ext) = file_name.rsplit_once('.').unwrap_or((file_name, "txt"));

    // Hash archive + entry so identically-named classes from different jars don't
    // collide (e.g. two `Strings.kt` from different source sets).
    let mut hasher = DefaultHasher::new();
    archive_path.hash(&mut hasher);
    inner.hash(&mut hasher);
    let out_path = cache_dir().join(format!("{stem}-{:016x}.{ext}", hasher.finish()));

    if !out_path.exists() {
        let content = read_zip_entry(&archive_path, &inner)?;
        fs::create_dir_all(cache_dir()).ok()?;
        fs::write(&out_path, content).ok()?;
    }

    Some(path_to_file_uri(&out_path))
}

/// Split `…/foo.jar!/inner/File.kt` into (`…/foo.jar` with its scheme, `inner/File.kt`).
/// Matches both `.jar!/` and `.zip!/` case-insensitively.
fn split_archive_uri(uri: &str) -> Option<(&str, &str)> {
    let lower = uri.to_ascii_lowercase();
    // `.jar!/` / `.zip!/` are 6 bytes; `find` returns the index of the leading '.'.
    // Adding 4 lands on '!' — i.e. right after `.jar` / `.zip`. ASCII lowercasing
    // preserves byte length, so indices computed on `lower` are valid in `uri`.
    let sep = [".jar!/", ".zip!/"]
        .iter()
        .filter_map(|marker| lower.find(marker).map(|i| i + 4))
        .min()?;

    let archive = &uri[..sep];
    let inner = uri[sep..].strip_prefix("!/")?;
    Some((archive, inner))
}

/// Turn a `file://` archive URI into a filesystem path.
fn uri_to_path(archive_uri: &str) -> String {
    let stripped = archive_uri
        .strip_prefix("file://")
        .unwrap_or(archive_uri);
    let decoded = percent_decode(stripped);

    // On Windows a URI path is `/C:/…`; drop the leading slash.
    #[cfg(windows)]
    {
        return decoded
            .strip_prefix('/')
            .map(str::to_owned)
            .unwrap_or(decoded);
    }
    #[cfg(not(windows))]
    {
        decoded
    }
}

fn read_zip_entry(archive: &str, inner: &str) -> Option<Vec<u8>> {
    let file = fs::File::open(archive).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut entry = zip.by_name(inner).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn path_to_file_uri(path: &PathBuf) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

/// Minimal percent-decoding for `%XX` byte escapes found in file:// URIs.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_jar_uri() {
        let (archive, inner) = split_archive_uri(
            "file:///home/u/.gradle/caches/kotlin-stdlib-2.3.0-sources.jar!/jvmMain/kotlin/Collections.kt",
        )
        .unwrap();
        assert_eq!(
            archive,
            "file:///home/u/.gradle/caches/kotlin-stdlib-2.3.0-sources.jar"
        );
        assert_eq!(inner, "jvmMain/kotlin/Collections.kt");
    }

    #[test]
    fn splits_src_zip_uri() {
        let (archive, inner) =
            split_archive_uri("file:///opt/jdk/lib/src.zip!/java.base/java/time/Instant.java")
                .unwrap();
        assert_eq!(archive, "file:///opt/jdk/lib/src.zip");
        assert_eq!(inner, "java.base/java/time/Instant.java");
    }

    #[test]
    fn ignores_plain_files() {
        assert!(split_archive_uri("file:///home/u/project/src/Main.kt").is_none());
        assert!(resolve_archive_uri("file:///home/u/project/src/Main.kt").is_none());
    }

    #[test]
    fn strips_file_scheme() {
        assert_eq!(uri_to_path("file:///opt/jdk/lib/src.zip"), "/opt/jdk/lib/src.zip");
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
    }
}
