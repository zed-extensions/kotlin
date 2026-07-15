# Contributing

## Repository layout

| Path | Purpose |
|------|---------|
| `src/` | WASM extension (Zed loads this) |
| `proxy/` | Native `kotlin-lsp-proxy` binary |
| `docs/` | Design notes |
| `.github/workflows/` | CI and release automation |

## Local development

```bash
# Proxy unit tests
cd proxy && cargo test && cargo clippy -- -D warnings

# Install proxy into Zed extension workdir (macOS/Linux)
./setup-local-proxy.sh

# In Zed: Install Dev Extension → this repo
# Rebuild after WASM changes; restart LS after proxy-only changes
```

## Configuration

See [docs/JAR_SOURCE_PROXY.md](./docs/JAR_SOURCE_PROXY.md).

```json
{
  "lsp": {
    "kotlin-language-server": {
      "settings": {
        "sourceProxy": {
          "enabled": true,
          "debug": false
        }
      }
    }
  }
}
```

`sourceProxy` is stripped before settings are sent to the language server.

## Releasing proxy binaries

1. Bump `version` in `Cargo.toml` and `extension.toml` together.
2. Tag and publish a GitHub Release `vX.Y.Z`.
3. Workflow `release-proxy.yml` builds and attaches platform assets.

## Pull requests

- Keep the proxy **fail-open** (never take down the LS on rewrite failure).
- Add/adjust unit tests for new URI shapes.
- Run `cargo test --manifest-path proxy/Cargo.toml` before pushing.
