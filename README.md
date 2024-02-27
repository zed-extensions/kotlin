# zed-kotlin
Syntax highlighting for Kotlin in [Zed](https://github.com/zed-industries/zed).

### Test locally

- Clone this repo: `git clone https://github.com/evrsen/zed-kotlin kotlin`
- Clone the [tree-sitter-kotlin](https://github.com/fwcd/tree-sitter-kotlin) repo: `https://github.com/fwcd/tree-sitter-kotlin`
- CD into the repo: `cd tree-sitter-kotlin`
- Build the WASM: `tree-sitter build-wasm` (might require docker-engine running)
- Rename the WASM file to `kotlin.wasm`
- Move the WASM file into `kotlin/grammars` (this repository)
- Move the `kotlin`repository to `~/Library/Application Support/Zed/extensions/installed`
