# zed-kotlin

Kotlin language support for [Zed](https://github.com/zed-industries/zed).

## Features

- **Syntax** via tree-sitter Kotlin
- **Language servers**
  - [fwcd/kotlin-language-server](https://github.com/fwcd/kotlin-language-server) (default)
  - [Kotlin/kotlin-lsp](https://github.com/Kotlin/kotlin-lsp) (opt-in, pre-alpha)
- **Library source navigation**: optional `kotlin-lsp-proxy` rewrites `jar://` /
  `*.jar!` definition targets so Zed can open extracted sources  
  → [docs/JAR_SOURCE_PROXY.md](./docs/JAR_SOURCE_PROXY.md)

## Language Servers

### Kotlin Language Server (default)

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

## Library sources (`jar://`)

When the language server returns locations inside dependency source jars, Zed
may open an empty tab. This extension can run a small proxy in front of the LS
to extract those entries first.

| | |
|--|--|
| Enable (default) | proxy binary present / auto-downloaded |
| Disable | `"sourceProxy": { "enabled": false }` or `KOTLIN_LSP_PROXY_DISABLE=1` |
| Debug | `"sourceProxy": { "debug": true }` or `KOTLIN_LSP_PROXY_DEBUG=1` |
| Local build | `./setup-local-proxy.sh` |

Details and security notes: [docs/JAR_SOURCE_PROXY.md](./docs/JAR_SOURCE_PROXY.md),
[SECURITY.md](./SECURITY.md).

## Developing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

```bash
cd proxy && cargo test
./setup-local-proxy.sh
# Zed: Install Dev Extension on this repo; rebuild after WASM changes
```
