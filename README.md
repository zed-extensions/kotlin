# zed-kotlin

Kotlin language support for [Zed](https://github.com/zed-industries/zed).

## Features

- **Syntax** via tree-sitter Kotlin
- **Language servers**
  - [fwcd/kotlin-language-server](https://github.com/fwcd/kotlin-language-server) (default)
  - [Kotlin/kotlin-lsp](https://github.com/Kotlin/kotlin-lsp) (opt-in, pre-alpha)
- **Library source navigation**: optional `kotlin-lsp-proxy` rewrites `jar://` /
  `*.jar!` definition targets so Zed can open extracted sources (see
  [docs/JAR_SOURCE_PROXY.md](./docs/JAR_SOURCE_PROXY.md))

## Language Servers

### Kotlin Language Server (default)

Workspace settings go under `lsp.kotlin-language-server` in `settings.json`:

```json
{
  "lsp": {
    "kotlin-language-server": {
      "settings": {
        "compiler": {
          "jvm": {
            "target": "17"
          }
        }
      }
    }
  }
}
```

Full options:
[Configuration.kt](https://github.com/fwcd/kotlin-language-server/blob/main/server/src/main/kotlin/org/javacs/kt/Configuration.kt).

### Kotlin LSP (JetBrains, pre-alpha)

```json
{
  "languages": {
    "Kotlin": {
      "language_servers": ["kotlin-lsp"]
    }
  }
}
```

Optional custom binary:

```json
{
  "lsp": {
    "kotlin-lsp": {
      "binary": {
        "path": "path/to/kotlin-lsp.sh",
        "arguments": ["--stdio"]
      }
    }
  }
}
```

## Library sources (`jar://`)

When the language server returns locations inside dependency source jars, Zed
historically opened an empty tab. This extension can run a small proxy in front
of the LS to extract those entries to disk first.

- Auto-download: platform binaries from the extension release tag `v{version}`
- Local build: `./setup-local-proxy.sh`
- Disable: `KOTLIN_LSP_PROXY_DISABLE=1`
- Debug log: `KOTLIN_LSP_PROXY_DEBUG=1` → `~/.cache/zed-kotlin-jar-sources/proxy.log`

Details: [docs/JAR_SOURCE_PROXY.md](./docs/JAR_SOURCE_PROXY.md).

## Developing this extension

```bash
# Install as a Zed dev extension (point at this repo)
# Proxy for local jar-navigation testing:
./setup-local-proxy.sh

cd proxy && cargo test
```

After WASM changes: **extensions: rebuild dev extension** in Zed.  
After proxy-only changes: re-run `./setup-local-proxy.sh` and restart the LS.
