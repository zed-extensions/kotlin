//! kotlin-lsp-proxy — stdio proxy for the Zed Kotlin extension.
//!
//! Rewrites archive URIs (`jar:///…`, `file://…!/…`, `%21`, etc.) to extracted
//! files under the user cache so Zed can open library sources.
//!
//! ## Fail-open
//! - Rewrite failures leave the original URI.
//! - Panic/JSON errors while rewriting → forward the raw message.
//! - `KOTLIN_LSP_PROXY_DISABLE=1` → exec the real language server (no proxying).
//! - If the real LS cannot be spawned for proxying, try `exec` as last resort.
//!
//! ## Logging
//! - stderr (Zed: language-server stderr)
//! - `~/.cache/zed-kotlin-jar-sources/proxy.log` (or `$KOTLIN_LSP_PROXY_LOG`)
//! - `KOTLIN_LSP_PROXY_DEBUG=1` for URI before/after and message snippets

mod cache;
mod logutil;
mod lsp_frame;
mod rewrite;
mod security;
mod uri;

use logutil::{debug_enabled, env_flag, log_line};
use lsp_frame::{frame_json, lsp_body, parse_lsp_content, write_raw, LspReader};
use rewrite::rewrite_archive_uris;
use uri::body_has_archive_uri;

use std::{
    env, fs,
    io::BufReader,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

fn main() {
    let mut args = env::args().skip(1);
    let Some(bin_arg) = args.next() else {
        eprintln!("Usage: kotlin-lsp-proxy <language-server-binary> [args...]");
        process::exit(2);
    };
    let child_args: Vec<String> = args.collect();
    let debug = debug_enabled();

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
            let mut reader = LspReader::new(BufReader::new(std::io::stdin()));
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

    // server -> Zed
    {
        let alive_out = Arc::clone(&alive);
        let mut reader = LspReader::new(BufReader::new(child_stdout));
        while alive_out.load(Ordering::Relaxed) {
            match reader.read_message() {
                Ok(Some(raw)) => write_maybe_rewritten(&raw, debug),
                Ok(None) | Err(_) => break,
            }
        }
        alive_out.store(false, Ordering::Relaxed);
    }

    log_line("exit");
    let _ = child.wait();
}

use std::io::Write as _;

fn write_maybe_rewritten(raw: &[u8], debug: bool) {
    let Some(body) = lsp_body(raw) else {
        write_raw(raw);
        return;
    };
    if !body_has_archive_uri(body) {
        write_raw(raw);
        return;
    }

    if debug {
        let s = String::from_utf8_lossy(body);
        let snippet: String = s.chars().take(800).collect();
        log_line(&format!("archive-msg snippet: {snippet}"));
    }

    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| try_rewrite_message(raw)));

    match result {
        Ok(Some(rewritten)) => write_raw(&rewritten),
        Ok(None) => write_raw(raw),
        Err(_) => {
            log_line("WARN panic during rewrite; forwarding original message");
            write_raw(raw);
        }
    }
}

/// Returns framed bytes if at least one URI was rewritten; otherwise `None`.
fn try_rewrite_message(raw: &[u8]) -> Option<Vec<u8>> {
    let mut msg = parse_lsp_content(raw)?;
    let rewrites = rewrite_archive_uris(&mut msg);
    log_line(&format!("rewritten {rewrites} archive URI(s)"));
    if rewrites == 0 {
        return None;
    }
    frame_json(&msg)
}

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

fn resolve_language_server_binary(bin: &str) -> PathBuf {
    let p = PathBuf::from(bin);
    if p.is_absolute() {
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
