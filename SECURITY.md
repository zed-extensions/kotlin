# Security policy

## kotlin-lsp-proxy

The optional jar/zip source proxy extracts entries from archives referenced by
language-server URIs into a user cache directory.

### Protections

- **Zip-slip**: archive member names with `..` or absolute paths are rejected.
- **Archive allowlist** (default): only archives under the user home, `JAVA_HOME`,
  or common JDK install prefixes, with jar/zip-like names. Override with
  `KOTLIN_LSP_PROXY_ALLOW_ANY_ARCHIVE=1` if you use unusual repository layouts.
- **Fail-open**: extraction failures do not execute code from the archive; the
  original URI is left unchanged.
- **No network**: the proxy does not download artifacts; it only reads local paths
  provided by the language server.

### Reporting issues

Please open a private security advisory or contact the maintainers of
[zed-extensions/kotlin](https://github.com/zed-extensions/kotlin) for sensitive
reports. For non-sensitive bugs, use public issues.
