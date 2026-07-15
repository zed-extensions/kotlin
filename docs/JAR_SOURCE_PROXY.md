# Library source navigation (`jar://` / `zip!`)

## Problem

Kotlin language servers (especially JetBrains **kotlin-lsp**) often return
definition locations like:

```text
jar:///Users/…/.m2/…/spring-web-…-sources.jar!/org/…/RequestParam.java
```

Zed treats that string as a normal filesystem path. There is no file with a `!`
in the name, so the buffer opens **empty**.

This is a known class of editor issue with archive URIs (`path.jar!/entry`).

## Solution

A small **stdio LSP proxy** sits between Zed and the real language server:

```text
Zed  ↔  kotlin-lsp-proxy  ↔  kotlin-language-server | kotlin-lsp
```

When a response contains an archive URI, the proxy:

1. Parses `archive!entry` (and `jar:///`, `file://`, `%21`, etc.)
2. Validates paths (zip-slip protection, archive allowlist)
3. Extracts the entry into `~/.cache/zed-kotlin-jar-sources/`
4. Rewrites the Location URI to `file:///…/cache/…`

If extraction fails, the **original URI is kept** (fail-open: LS stays alive).

## Architecture

| Component | Role |
|-----------|------|
| WASM extension (`src/kotlin.rs`) | Chooses the process Zed starts; wraps both Kotlin LSes when a proxy binary is available |
| `kotlin-lsp-proxy` (native) | Long-lived process that rewrites archive locations |

The WASM sandbox cannot sit on the LSP stream or reliably open jars; a native
helper is the same pattern as the Java extension’s `java-lsp-proxy`.

### Proxy crate modules

| Module | Responsibility |
|--------|----------------|
| `main.rs` | Process lifecycle, stdio bridge |
| `lsp_frame.rs` | `Content-Length` framing |
| `uri.rs` | URI parse + zip extract |
| `security.rs` | Zip-slip + archive allowlist |
| `cache.rs` | Cache paths + size-based GC |
| `rewrite.rs` | JSON tree walk |
| `logutil.rs` | stderr + file logging |

## Installation of the proxy binary

1. **GitHub Release** (production): assets named  
   `kotlin-lsp-proxy-{darwin,linux,windows}-{aarch64,x86_64}.tar.gz|.zip`  
   on tag `v{extension version}`. The extension downloads them automatically.
2. **Local dev**: `./setup-local-proxy.sh`
3. **Override**: `KOTLIN_LSP_PROXY=/path/to/binary`
4. **Disable (env)**: `KOTLIN_LSP_PROXY_DISABLE=1`
5. **Disable (settings)**:

```json
{
  "lsp": {
    "kotlin-language-server": {
      "settings": {
        "sourceProxy": {
          "enabled": false
        }
      }
    }
  }
}
```

Optional debug:

```json
"sourceProxy": { "enabled": true, "debug": true }
```

or `KOTLIN_LSP_PROXY_DEBUG=1` → logs under `~/.cache/zed-kotlin-jar-sources/proxy.log`.

Fork testing before assets exist on `zed-extensions/kotlin`:

```bash
export KOTLIN_LSP_PROXY_GITHUB_REPO=you/kotlin
# or use setup-local-proxy.sh
```

## Environment variables

| Variable | Purpose |
|----------|---------|
| `KOTLIN_LSP_PROXY` | Absolute path to proxy binary |
| `KOTLIN_LSP_PROXY_DISABLE` | Skip proxy entirely |
| `KOTLIN_LSP_PROXY_DEBUG` | Verbose rewrite logging |
| `KOTLIN_LSP_PROXY_LOG` | Log file path |
| `KOTLIN_LSP_PROXY_CACHE` | Cache directory |
| `KOTLIN_LSP_PROXY_CACHE_MAX_MB` | Cache budget (default 512) |
| `KOTLIN_LSP_PROXY_ALLOW_ANY_ARCHIVE` | Disable archive path allowlist |
| `KOTLIN_LSP_PROXY_GITHUB_REPO` | Override release download repo |

## Supported URI forms

- `/abs/path/foo.jar!/entry`
- `file:///abs/path/foo.jar!/entry`
- `jar:file:///abs/path/foo.jar!/entry`
- `jar:///abs/path/foo.jar!/entry` (**kotlin-lsp**)
- `%21` instead of `!`
- `.zip!` (e.g. JDK `src.zip`)

## Security

See [SECURITY.md](../SECURITY.md). Summary: zip-slip rejected; default allowlist for
home/JDK trees; no network I/O in the proxy process.

## Developing the proxy

```bash
cd proxy && cargo test && cargo clippy -- -D warnings
../setup-local-proxy.sh
```

No WASM rebuild is required for proxy-only changes.
