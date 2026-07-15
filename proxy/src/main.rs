//! Local-only LSP proxy for Kotlin language servers.
//!
//! Rewrites archive URIs (`jar:///…`, `file://…!/…`, `%21`, etc.) to extracted
//! files under `~/.cache/zed-kotlin-jar-sources/` so Zed can open them.
//!
//! ## Fail-open design
//! - Rewrite failures leave the original URI (LS still works; only that jump is empty).
//! - Panic/JSON errors while rewriting → forward the raw LSP message unchanged.
//! - `KOTLIN_LSP_PROXY_DISABLE=1` → become the real LS (no proxying).
//! - If the real LS cannot be spawned for proxying, try `exec` of the LS as a last resort.
//!
//! ## Logging
//! - stderr (Zed: "server stderr")
//! - `~/.cache/zed-kotlin-jar-sources/proxy.log` (or `$KOTLIN_LSP_PROXY_LOG`)
//! - `KOTLIN_LSP_PROXY_DEBUG=1` → URI before/after + snippets

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const CONTENT_LENGTH: &str = "Content-Length";
const HEADER_SEP: &[u8] = b"\r\n\r\n";

fn main() {
    let mut args = env::args().skip(1);
    let Some(bin_arg) = args.next() else {
        eprintln!("Usage: kotlin-lsp-proxy <language-server-binary> [args...]");
        process::exit(2);
    };
    let child_args: Vec<String> = args.collect();
    let debug = env_flag("KOTLIN_LSP_PROXY_DEBUG");

    log_line(&format!(
        "start debug={debug} disable={} pid={} cwd={:?} arg0={bin_arg} rest={child_args:?}",
        env_flag("KOTLIN_LSP_PROXY_DISABLE"),
        process::id(),
        env::current_dir().ok()
    ));

    let bin = resolve_language_server_binary(&bin_arg);
    if !bin.is_file() {
        log_line(&format!(
            "ERROR language server not found given={bin_arg} resolved={} cwd={:?}",
            bin.display(),
            env::current_dir().ok()
        ));
        process::exit(1);
    }

    // Emergency: skip all proxying and become the real language server.
    if env_flag("KOTLIN_LSP_PROXY_DISABLE") {
        log_line(&format!(
            "KOTLIN_LSP_PROXY_DISABLE=1 — exec LS without rewrite: {}",
            bin.display()
        ));
        become_language_server(&bin, &child_args);
    }

    log_line(&format!("spawning (proxied) {}", bin.display()));

    let mut child = match Command::new(&bin)
        .args(&child_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            // Fail-open: try to hand stdio to the real LS so the editor still works.
            log_line(&format!(
                "WARN spawn failed ({e}); falling back to exec LS without rewrite"
            ));
            become_language_server(&bin, &child_args);
        }
    };

    let child_stdin = Arc::new(Mutex::new(child.stdin.take().expect("stdin")));
    let child_stdout = child.stdout.take().expect("stdout");
    let alive = Arc::new(AtomicBool::new(true));

    // Zed -> server (pass-through)
    {
        let writer = Arc::clone(&child_stdin);
        let alive_in = Arc::clone(&alive);
        thread::spawn(move || {
            let mut reader = LspReader::new(BufReader::new(io::stdin()));
            while alive_in.load(Ordering::Relaxed) {
                match reader.read_message() {
                    Ok(Some(raw)) => {
                        let mut w = writer.lock().unwrap();
                        if w.write_all(&raw).is_err() || w.flush().is_err() {
                            break;
                        }
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            alive_in.store(false, Ordering::Relaxed);
        });
    }

    // server -> Zed (best-effort rewrite; never drop a message on rewrite failure)
    {
        let alive_out = Arc::clone(&alive);
        let mut reader = LspReader::new(BufReader::new(child_stdout));
        while alive_out.load(Ordering::Relaxed) {
            match reader.read_message() {
                Ok(Some(raw)) => {
                    write_maybe_rewritten(&raw, debug);
                }
                Ok(None) | Err(_) => break,
            }
        }
        alive_out.store(false, Ordering::Relaxed);
    }

    log_line("exit");
    let _ = child.wait();
}

/// Rewrite archive URIs if present; on any failure forward `raw` unchanged.
fn write_maybe_rewritten(raw: &[u8], debug: bool) {
    if !body_has_archive_uri(raw) {
        write_raw(raw);
        return;
    }

    if debug {
        if let Some(body) = lsp_body(raw) {
            let s = String::from_utf8_lossy(body);
            let snippet: String = s.chars().take(800).collect();
            log_line(&format!("archive-msg snippet: {snippet}"));
        }
    }

    // Isolate panics so a bad URI never kills the LS process.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        try_rewrite_message(raw, debug)
    }));

    match result {
        Ok(Some(rewritten_json)) => {
            write_raw(&rewritten_json);
        }
        Ok(None) => {
            // Intentional pass-through (parse fail, zero rewrites with need to keep original, etc.)
            write_raw(raw);
        }
        Err(_) => {
            log_line("WARN panic during rewrite; forwarding original message");
            write_raw(raw);
        }
    }
}

/// Returns framed LSP bytes if rewrite succeeded and produced changes; None → use original.
fn try_rewrite_message(raw: &[u8], debug: bool) -> Option<Vec<u8>> {
    let mut msg = parse_lsp_content(raw)?;

    let rewrites = rewrite_archive_uris(&mut msg, debug);
    log_line(&format!("rewritten {rewrites} archive URI(s)"));

    // If nothing was rewritten (materialize failed or no archive strings), keep
    // the original framed bytes so we do not re-serialize needlessly.
    if rewrites == 0 {
        return None;
    }

    let json = serde_json::to_string(&msg).ok()?;
    let framed = format!("{CONTENT_LENGTH}: {}\r\n\r\n{json}", json.len());
    Some(framed.into_bytes())
}

/// Replace this process with the real language server (stdio already connected to Zed).
fn become_language_server(bin: &Path, args: &[String]) -> ! {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = Command::new(bin).args(args).exec();
        log_line(&format!("ERROR exec {} failed: {err}", bin.display()));
        process::exit(1);
    }
    #[cfg(not(unix))]
    {
        let status = Command::new(bin)
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        match status {
            Ok(s) => process::exit(s.code().unwrap_or(1)),
            Err(e) => {
                log_line(&format!("ERROR run {} failed: {e}", bin.display()));
                process::exit(1);
            }
        }
    }
}

fn env_flag(name: &str) -> bool {
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

fn log_line(msg: &str) {
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

fn resolve_language_server_binary(bin: &str) -> PathBuf {
    let p = PathBuf::from(bin);
    if p.is_absolute() {
        if p.is_file() {
            return p;
        }
        // absolute but missing — still return for error message
        return p;
    }

    if let Ok(exe) = env::current_exe() {
        let exe = fs::canonicalize(&exe).unwrap_or(exe);
        if let Some(bin_dir) = exe.parent() {
            if let Some(work_dir) = bin_dir.parent() {
                let candidate = work_dir.join(bin);
                if candidate.is_file() {
                    return candidate;
                }
            }
            let candidate = bin_dir.join(bin);
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    if let Ok(cwd) = env::current_dir() {
        let candidate = cwd.join(bin);
        if candidate.is_file() {
            return candidate;
        }
    }

    p
}

fn body_has_archive_uri(raw: &[u8]) -> bool {
    let Some(body) = lsp_body(raw) else {
        return false;
    };
    contains_subslice(body, b".jar!")
        || contains_subslice(body, b".zip!")
        || contains_subslice(body, b".JAR!")
        || contains_subslice(body, b".ZIP!")
        || contains_subslice(body, b".jar%21")
        || contains_subslice(body, b".zip%21")
        || contains_subslice(body, b".jar%2521")
        || contains_subslice(body, b"jar:file:")
        || contains_subslice(body, b"jar://")
}

fn rewrite_archive_uris(value: &mut Value, debug: bool) -> usize {
    match value {
        Value::String(s) => {
            if looks_like_archive_uri(s) {
                match materialize_archive_uri(s) {
                    Some(rewritten) => {
                        if debug {
                            log_line(&format!("rewrite\n  from: {s}\n  to:   {rewritten}"));
                        } else {
                            log_line(&format!(
                                "rewrite ok -> {}",
                                Path::new(rewritten.trim_start_matches("file://"))
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("?")
                            ));
                        }
                        *s = rewritten;
                        1
                    }
                    None => {
                        // Fail-open: keep original string (Zed may show empty jar tab).
                        log_line(&format!("rewrite FAILED (kept original): {s}"));
                        0
                    }
                }
            } else {
                0
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .map(|v| rewrite_archive_uris(v, debug))
            .sum(),
        Value::Object(map) => map
            .values_mut()
            .map(|v| rewrite_archive_uris(v, debug))
            .sum(),
        _ => 0,
    }
}

fn looks_like_archive_uri(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    (lower.contains(".jar") || lower.contains(".zip") || lower.starts_with("jar:"))
        && (s.contains('!') || lower.contains("%21"))
}

fn materialize_archive_uri(uri: &str) -> Option<String> {
    let (archive_path, entry) = split_archive_ref(uri)?;
    if entry.is_empty() {
        log_line(&format!("empty entry for uri={uri}"));
        return None;
    }
    if !archive_path.is_file() {
        log_line(&format!(
            "archive missing: {} (uri={uri})",
            archive_path.display()
        ));
        return None;
    }

    let cache_file = cache_file_for(&archive_path, &entry);
    if !cache_file.is_file() {
        if let Err(e) = extract_entry(&archive_path, &entry, &cache_file) {
            log_line(&format!(
                "extract failed {}!{}: {e}",
                archive_path.display(),
                entry
            ));
            return None;
        }
        log_line(&format!(
            "extracted {}!{} -> {}",
            archive_path.display(),
            entry,
            cache_file.display()
        ));
    }

    // Refuse empty / unreadable extracts so we don't hand Zed a blank buffer.
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

fn split_archive_ref(uri: &str) -> Option<(PathBuf, String)> {
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

    // kotlin-lsp: jar:///abs/path/to.jar!/entry
    // also: jar:file:///… , file:///…!/…
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
        if let Some(home) = env::var_os("HOME") {
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

fn cache_root() -> PathBuf {
    if let Ok(dir) = env::var("KOTLIN_LSP_PROXY_CACHE") {
        return PathBuf::from(dir);
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("zed-kotlin-jar-sources");
    }
    env::temp_dir().join("zed-kotlin-jar-sources")
}

fn cache_file_for(archive: &Path, entry: &str) -> PathBuf {
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
    let digest = hex::encode(hasher.finalize());
    cache_root().join(&digest[..16]).join(entry)
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let bytes = bytes.as_ref();
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0xf) as usize] as char);
        }
        s
    }
}

fn extract_entry(archive: &Path, entry: &str, dest: &Path) -> io::Result<()> {
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

    let mut zf = zip.by_name(&name).map_err(io_err)?;
    let mut out = fs::File::create(dest)?;
    io::copy(&mut zf, &mut out)?;
    Ok(())
}

fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
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

// --- minimal LSP framing ---

struct LspReader<R> {
    reader: R,
}

impl<R: Read> LspReader<R> {
    fn new(reader: R) -> Self {
        Self { reader }
    }

    fn read_message(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut header_buf = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            match self.reader.read(&mut byte)? {
                0 => return Ok(None),
                _ => header_buf.push(byte[0]),
            }
            if header_buf.ends_with(HEADER_SEP) {
                break;
            }
        }

        let content_length = parse_content_length(&header_buf);
        let mut content = vec![0u8; content_length];
        self.reader.read_exact(&mut content)?;

        let mut message = header_buf;
        message.extend_from_slice(&content);
        Ok(Some(message))
    }
}

fn parse_content_length(header: &[u8]) -> usize {
    String::from_utf8_lossy(header)
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case(CONTENT_LENGTH) {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn lsp_body(raw: &[u8]) -> Option<&[u8]> {
    let sep = raw.windows(4).position(|w| w == HEADER_SEP)?;
    Some(&raw[sep + 4..])
}

fn parse_lsp_content(raw: &[u8]) -> Option<Value> {
    serde_json::from_slice(lsp_body(raw)?).ok()
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn write_raw(raw: &[u8]) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(raw);
    let _ = out.flush();
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
        let home = env::var("HOME").unwrap();
        let jar = format!(
            "{home}/.m2/repository/org/springframework/spring-web/7.0.8/spring-web-7.0.8-sources.jar"
        );
        if !Path::new(&jar).is_file() {
            return;
        }
        let uri = format!(
            "jar://{jar}!/org/springframework/web/bind/annotation/RequestParam.java"
        );
        assert!(
            uri.starts_with("jar:///"),
            "expected jar:///… when jar is absolute, got {uri}"
        );
        let out = materialize_archive_uri(&uri).expect("materialize jetbrains uri");
        assert!(out.starts_with("file://"));
        let path = out.trim_start_matches("file://");
        let body = fs::read_to_string(path).expect("read");
        assert!(body.contains("interface RequestParam"));
    }

    #[test]
    fn materializes_spring_sources_if_present() {
        let home = env::var("HOME").unwrap();
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

    #[test]
    fn rewrite_json_location_result() {
        let home = env::var("HOME").unwrap();
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
        let mut msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": [{ "uri": uri, "range": { "start": {"line":0,"character":0}, "end": {"line":0,"character":0} } }]
        });
        let n = rewrite_archive_uris(&mut msg, false);
        assert_eq!(n, 1);
        let new_uri = msg["result"][0]["uri"].as_str().unwrap();
        assert!(new_uri.starts_with("file://"));
        assert!(!new_uri.contains(".jar!"));
    }

    #[test]
    fn body_detects_jar_bang() {
        let body = br#"{"result":{"uri":"/x/foo-sources.jar!/a/B.java"}}"#;
        let framed = format!(
            "Content-Length: {}\r\n\r\n{}",
            body.len(),
            std::str::from_utf8(body).unwrap()
        );
        assert!(body_has_archive_uri(framed.as_bytes()));
    }

    #[test]
    fn body_detects_percent_21() {
        let body = br#"{"uri":"/x/foo.jar%21/a/B.java"}"#;
        let framed = format!(
            "Content-Length: {}\r\n\r\n{}",
            body.len(),
            std::str::from_utf8(body).unwrap()
        );
        assert!(body_has_archive_uri(framed.as_bytes()));
    }

    #[test]
    fn rewrite_failure_keeps_original_string() {
        let mut msg = serde_json::json!({
            "result": { "uri": "jar:///no/such/file.jar!/com/X.java" }
        });
        let original = msg["result"]["uri"].as_str().unwrap().to_string();
        let n = rewrite_archive_uris(&mut msg, false);
        assert_eq!(n, 0);
        assert_eq!(msg["result"]["uri"].as_str().unwrap(), original);
    }
}
