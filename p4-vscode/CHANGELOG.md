# Changelog

## 0.1.1 (2026-05-10)

### Added
- **Type checking**: Variable declaration type mismatch detection (`typer.rs`)
- **Rename**: `textDocument/rename` for local variables and parameters (`rename.rs`)
- **Semantic tokens**: Keyword / type / function / variable / number / string / comment coloring (`semantic_tokens.rs`)
- **Document symbols**: Outline view support for struct, control, parser, action, table (`index.rs`)
- **Signature help**: Server-side infrastructure (client trigger characters registered)
- **Diagnostics**: `undefined reference` detection for unresolved identifiers (`diagnostics.rs`)
- **`#include` resolution**: Basic include path parsing and cross-file indexing (`workspace.rs`)
- **E2E tests**: 8 passing integration tests covering hover, completion, goto definition, diagnostics

### Changed
- **Hover**: Now resolves extern method signatures and struct field chains from AST
- **Completion**: Dot-triggered field completion uses workspace type definitions
- **tree-sitter-p4 grammar**: Fixed `expr` binop rule (removed erroneous `optional($.expr)`)

### Fixed
- **Goto Definition**: Correctly jumps to struct/control/parser definitions
- **Workspace indexing**: `FileIndex` now stores `tree` and `source` for completion/hover reuse
- **Block locals**: `collect_block_locals` now recursively enters `stmt` nodes

## 0.1.0 (2026-05-08)

### Added
- Initial release with basic LSP features
- Syntax highlighting via TextMate grammar
- Real-time syntax error diagnostics via Tree-sitter
- Hover information for struct/header definitions
- Go to definition for top-level symbols
- Auto-completion for keywords and local variables
- Basic workspace symbol indexing

