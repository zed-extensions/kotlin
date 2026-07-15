//! kotlin-lsp-proxy — a thin stdio proxy in front of JetBrains `kotlin-lsp`.
//!
//! kotlin-lsp returns navigation targets that live inside source archives
//! (`…/foo-sources.jar!/pkg/Foo.kt`, `…/lib/src.zip!/…/Instant.java`). Zed cannot open
//! those archive-internal paths, so "Go to Definition" into any library/JDK class shows
//! an empty buffer (zed-extensions/kotlin#106).
//!
//! This proxy sits between Zed and kotlin-lsp, forwards every message untouched, and only
//! rewrites the location URIs in definition-family responses: it extracts the referenced
//! entry from the jar/zip to a real temp file and points Zed at that instead.
//!
//! Usage: `kotlin-lsp-proxy <kotlin-lsp-binary> [args...]`

#[macro_use]
mod log;
mod lsp;
mod platform;
mod resolve;

use lsp::{parse_lsp_content, raw_has_id, write_raw, write_to_stdout, LspReader};
use platform::spawn_parent_monitor;
use resolve::rewrite_archive_locations;
use serde_json::Value;
use std::{
    collections::HashSet,
    env,
    io::{self, BufReader, Write},
    process::{self, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: kotlin-lsp-proxy <bin> [args...]");
        process::exit(1);
    }

    let bin = &args[0];
    let child_args = &args[1..];

    lsp_info!("kotlin-lsp-proxy starting: bin={bin}");

    let mut child = Command::new(bin)
        .args(child_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("Failed to spawn {bin}: {e}");
            lsp_error!("Failed to spawn {bin}: {e}");
            process::exit(1);
        });

    lsp_info!("kotlin-lsp process spawned (pid={})", child.id());

    let child_stdin = Arc::new(Mutex::new(child.stdin.take().unwrap()));
    let child_stdout = child.stdout.take().unwrap();
    let alive = Arc::new(AtomicBool::new(true));

    // Request ids whose responses may carry archive (`jar!/`) location URIs.
    let tracked: Arc<Mutex<HashSet<Value>>> = Arc::new(Mutex::new(HashSet::new()));

    // --- Thread 1: Zed stdin -> kotlin-lsp stdin (record definition-family ids) ---
    let stdin_writer = Arc::clone(&child_stdin);
    let alive_in = Arc::clone(&alive);
    let tracked_in = Arc::clone(&tracked);
    thread::spawn(move || {
        let stdin = io::stdin().lock();
        let mut reader = LspReader::new(BufReader::new(stdin));
        while alive_in.load(Ordering::Relaxed) {
            match reader.read_message() {
                Ok(Some(raw)) => {
                    // Only requests carry an `id`; skip parsing high-volume notifications.
                    if raw_has_id(&raw) {
                        if let Some(msg) = parse_lsp_content(&raw) {
                            if is_location_request(&msg) {
                                if let Some(id) = msg.get("id").cloned() {
                                    tracked_in.lock().unwrap().insert(id);
                                }
                            }
                        }
                    }
                    let mut w = stdin_writer.lock().unwrap();
                    if w.write_all(&raw).is_err() || w.flush().is_err() {
                        break;
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
        alive_in.store(false, Ordering::Relaxed);
    });

    // --- Thread 2: kotlin-lsp stdout -> rewrite archive URIs -> Zed stdout ---
    let alive_out = Arc::clone(&alive);
    let tracked_out = Arc::clone(&tracked);
    thread::spawn(move || {
        let mut reader = LspReader::new(BufReader::new(child_stdout));
        while alive_out.load(Ordering::Relaxed) {
            match reader.read_message() {
                Ok(Some(raw)) => {
                    // Notifications can't be responses we track: forward untouched.
                    if !raw_has_id(&raw) {
                        write_raw(&mut io::stdout().lock(), &raw);
                        continue;
                    }

                    let Some(mut msg) = parse_lsp_content(&raw) else {
                        write_raw(&mut io::stdout().lock(), &raw);
                        continue;
                    };

                    let is_tracked = msg
                        .get("id")
                        .map(|id| tracked_out.lock().unwrap().remove(id))
                        .unwrap_or(false);

                    if is_tracked && rewrite_archive_locations(&mut msg) {
                        write_to_stdout(&msg);
                    } else {
                        write_raw(&mut io::stdout().lock(), &raw);
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
        alive_out.store(false, Ordering::Relaxed);
    });

    // --- Thread 3: terminate kotlin-lsp if Zed dies ---
    spawn_parent_monitor(Arc::clone(&alive), child.id());

    let status = child.wait();
    lsp_info!("kotlin-lsp process exited: {status:?}");
    alive.store(false, Ordering::Relaxed);
}

fn is_location_request(msg: &Value) -> bool {
    matches!(
        msg.get("method").and_then(|m| m.as_str()),
        Some(
            "textDocument/definition"
                | "textDocument/typeDefinition"
                | "textDocument/implementation"
                | "textDocument/declaration"
                | "textDocument/references"
        )
    )
}
