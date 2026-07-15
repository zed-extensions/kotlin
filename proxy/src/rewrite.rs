//! Walk LSP JSON and rewrite archive URI strings.

use serde_json::Value;

use crate::logutil::{debug_enabled, log_line};
use crate::uri::{looks_like_archive_uri, materialize_archive_uri};

/// Rewrite archive URIs in-place. Returns how many strings were changed.
pub fn rewrite_archive_uris(value: &mut Value) -> usize {
    let debug = debug_enabled();
    rewrite_inner(value, debug)
}

fn rewrite_inner(value: &mut Value, debug: bool) -> usize {
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
                                std::path::Path::new(rewritten.trim_start_matches("file://"))
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("?")
                            ));
                        }
                        *s = rewritten;
                        1
                    }
                    None => {
                        log_line(&format!("rewrite FAILED (kept original): {s}"));
                        0
                    }
                }
            } else {
                0
            }
        }
        Value::Array(items) => items.iter_mut().map(|v| rewrite_inner(v, debug)).sum(),
        Value::Object(map) => map.values_mut().map(|v| rewrite_inner(v, debug)).sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn rewrite_json_location_result() {
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
        let mut msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": [{ "uri": uri, "range": { "start": {"line":0,"character":0}, "end": {"line":0,"character":0} } }]
        });
        let n = rewrite_archive_uris(&mut msg);
        assert_eq!(n, 1);
        let new_uri = msg["result"][0]["uri"].as_str().unwrap();
        assert!(new_uri.starts_with("file://"));
        assert!(!new_uri.contains(".jar!"));
    }

    #[test]
    fn rewrite_failure_keeps_original_string() {
        let mut msg = serde_json::json!({
            "result": { "uri": "jar:///no/such/file.jar!/com/X.java" }
        });
        let original = msg["result"]["uri"].as_str().unwrap().to_string();
        let n = rewrite_archive_uris(&mut msg);
        assert_eq!(n, 0);
        assert_eq!(msg["result"]["uri"].as_str().unwrap(), original);
    }
}
