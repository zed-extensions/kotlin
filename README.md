# zed-kotlin

Kotlin language support for [Zed](https://github.com/zed-industries/zed).

## Language Servers

### Kotlin Language Server

The [Kotlin Language Server](https://github.com/fwcd/kotlin-language-server) is an unofficial LSP for Kotlin, it is currently the most stable and popular language server for Kotlin. It is enabled by default by this extension.

#### Configuration

Workspace configuration options can be passed to the language server via lsp
settings in `settings.json`.

The following example changes the JVM target from `default` (which is 1.8) to
`17`:

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

The full list of workspace configuration options can be found
[here](https://github.com/fwcd/kotlin-language-server/blob/main/server/src/main/kotlin/org/javacs/kt/Configuration.kt).

### Kotlin LSP

[Kotlin LSP](https://github.com/kotlin/kotlin-lsp) is an official LSP implementation for Kotlin, built by JetBrains. It is currently pre-alpha.

#### Configuration

To use Kotlin LSP instead of the Kotlin Language Server, you must explicity enable it in your `settings.json`:

```json
{
  "languages": {
    "Kotlin": {
      "language_servers": ["kotlin-lsp"]
    }
  }
}
```

It will be downloaded and updated automatically when enabled, however, you can use a manually installed version by setting the path to the `kotlin-lsp.sh` script in the release assets:

```json
{
  "lsp": {
    "kotlin-lsp": {
      "binary": {
        "path": "path/to/kotlin-lsp.sh",
        "arguments": [ "--stdio" ]
      }
    }
  }
}
```

Note that the `kotlin-lsp.sh` script expects to be run from within the unzipped release zip file, and should not be moved elsewhere.

Alternatively, you can specify a custom download URL for the Kotlin LSP zip archive:

```json
{
  "lsp": {
    "kotlin-lsp": {
      "settings": {
        "download_url": "https://example.com/path/to/kotlin-lsp.zip"
      }
    }
  }
}
```
