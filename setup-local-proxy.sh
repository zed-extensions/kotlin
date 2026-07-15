#!/usr/bin/env bash
# Build the jar-URI proxy and install it into Zed's extension work directory.
# Use this for local development before GitHub Releases publish binaries.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
WORK="${HOME}/Library/Application Support/Zed/extensions/work/kotlin"
# Linux fallback
if [[ "$(uname -s)" == "Linux" ]]; then
  WORK="${XDG_DATA_HOME:-$HOME/.local/share}/zed/extensions/work/kotlin"
fi

echo "Building kotlin-lsp-proxy (release)..."
(cd "$ROOT/proxy" && cargo build --release)

mkdir -p "$WORK/bin"
BIN_NAME="kotlin-lsp-proxy"
if [[ "$(uname -s)" == "MINGW"* || "$(uname -s)" == "MSYS"* ]]; then
  BIN_NAME="kotlin-lsp-proxy.exe"
fi

cp "$ROOT/proxy/target/release/${BIN_NAME}" "$WORK/bin/${BIN_NAME}"
chmod +x "$WORK/bin/${BIN_NAME}" 2>/dev/null || true

echo "Installed: $WORK/bin/${BIN_NAME}"
echo "Next: in Zed, rebuild the dev extension (if needed) and restart the language server."
