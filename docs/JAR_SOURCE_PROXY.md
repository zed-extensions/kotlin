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
2. Extracts the entry into `~/.cache/zed-kotlin-jar-sources/`
3. Rewrites the Location URI to `file:///…/cache/…`

If extraction fails, the **original URI is kept** (fail-open: LS stays alive).

## Architecture

| Component | Role |
|-----------|------|
| WASM extension (`src/kotlin.rs`) | Chooses the process Zed starts; wraps both Kotlin LSes when a proxy binary is available |
| `kotlin-lsp-proxy` (native) | Long-lived process that rewrites archive locations |

The WASM sandbox cannot sit on the LSP stream or reliably open jars; a native
helper is the same pattern as the Java extension’s `java-lsp-proxy`.

## Installation of the proxy binary

1. **GitHub Release** (production): assets named  
   `kotlin-lsp-proxy-{darwin,linux,windows}-{aarch64,x86_64}.tar.gz|.zip`  
   on tag `v{extension version}`. The extension downloads them automatically.
2. **Local dev**: `./setup-local-proxy.sh` copies a release build into  
   `…/extensions/work/kotlin/bin/kotlin-lsp-proxy`.
3. **Override**: `KOTLIN_LSP_PROXY=/path/to/binary`
4. **Disable**: `KOTLIN_LSP_PROXY_DISABLE=1`

Optional: `KOTLIN_LSP_PROXY_DEBUG=1` logs URI before/after to stderr and  
`~/.cache/zed-kotlin-jar-sources/proxy.log`.

Fork testing (before assets exist on `zed-extensions/kotlin`):

```bash
export KOTLIN_LSP_PROXY_GITHUB_REPO=you/kotlin
# or use setup-local-proxy.sh
```

## Supported URI forms

- `/abs/path/foo.jar!/entry`
- `file:///abs/path/foo.jar!/entry`
- `jar:file:///abs/path/foo.jar!/entry`
- `jar:///abs/path/foo.jar!/entry` (JetBrains kotlin-lsp)
- `%21` instead of `!`
- `.zip!` (e.g. JDK `src.zip`)

## Developing the proxy

```bash
cd proxy && cargo test && cargo build --release
./setup-local-proxy.sh
# Zed: restart language server
```

No WASM rebuild is required for proxy-only changes.
