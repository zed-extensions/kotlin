# kotlin-lsp-proxy

Native stdio proxy used by the [Zed Kotlin extension](https://github.com/zed-extensions/kotlin).

It sits between Zed and a Kotlin language server, rewrites archive URIs
(`jar:///…/lib.jar!/path/File.java`) to extracted `file://` paths, and fail-opens
on errors so the language server keeps running.

```bash
cargo test
cargo build --release
```

See [../docs/JAR_SOURCE_PROXY.md](../docs/JAR_SOURCE_PROXY.md) for architecture and configuration.
