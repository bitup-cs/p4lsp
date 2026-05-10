# P4 Language Support for VSCode

P4-16 language support for Visual Studio Code via the `p4lsp-server` language server.

## Features

| Feature | Status | Description |
|---------|--------|-------------|
| Syntax highlighting | ✅ | TextMate grammar for P4-16 |
| Real-time diagnostics | ✅ | Syntax errors + undefined reference detection |
| Document symbols | ✅ | Outline view for headers, structs, controls, parsers, actions, tables |
| Hover information | ✅ | Type info on struct/header/enum/extern definitions and fields |
| Go to definition | ✅ | Jump to symbol definitions (structs, controls, parsers, actions) |
| Auto-completion | ✅ | Keywords, locals, struct fields, built-ins |
| Rename | ✅ | Rename local variables and parameters within scope |
| Type checking | ✅ | Type mismatch detection in variable declarations |
| Semantic tokens | ✅ | Keyword / type / function / variable / number / string coloring |
| Signature help | ⏸️ | Method signature hints (planned) |
| Find references | ⏸️ | Cross-file reference search (planned) |

## Known Issues

- **Document symbols** may show empty names for certain annotated constructs; filter in progress.
- **Rename** works for top-level definitions but has limited coverage inside `apply` blocks.
- **Signature help** and **find references** are not yet implemented.

## Requirements

- VSCode 1.85.0 or newer
- The bundled `p4lsp-server` binary is included for Linux x64
- For other platforms, set `p4lsp.serverPath` to a custom executable

## Installation

### From VSIX (local)

```bash
# Build the .vsix package
npm run package

# Install in VSCode
code --install-extension p4-vscode-0.1.1.vsix
```

### From Source (development)

```bash
cd p4-vscode
npm install
npm run compile
# Press F5 to launch Extension Host
```

## Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `p4lsp.serverPath` | `string \| null` | `null` | Path to p4lsp-server executable. If null, the bundled binary is used. |
| `p4lsp.includePaths` | `string[]` | `[]` | Additional include paths for P4 `#include` resolution. |
| `p4lsp.enableLogging` | `boolean` | `false` | Enable LSP request/response logging to the Output panel. |

## Usage

1. Open any `.p4` file in VSCode
2. The extension auto-activates and starts the language server
3. Enjoy syntax highlighting, completions, hover info, and more

## Development

```bash
# Compile TypeScript
npm run compile

# Watch mode
npm run watch

# Run integration tests
npm test

# Run tests in headless mode (CI)
npm run test:headless

# Build .vsix package
npm run package
```

### E2E Test Status

| Suite | Passing | Pending | Failing |
|-------|---------|---------|---------|
| P4 Language Server Integration | 8 | 5 | 0 |

**Passing:** Server binary, language mode, hover (struct/field), completion (keywords/dot-triggered), goto definition, diagnostics.

**Pending:** Extern method hover, rename (apply block limitation), document symbols (empty name filter), signature help, find references.

## Architecture

```
VSCode (UI)
  ↓
VSCode Extension Host (TypeScript client)
  ↓ stdio
p4lsp-server (Rust LSP server)
  ↓
tree-sitter-p4 (incremental parser)
```

## License

MIT OR Apache-2.0
