# P4LSP 架构设计

> 当前版本基于 Tree-sitter + Rust 实现，面向 P4-16 spec v1.2.5

---

## 总体架构

```
┌─────────────────────────────────────────────┐
│  p4-vscode (VS Code Extension)                │
│  ├── client/src/extension.ts   — 客户端入口   │
│  ├── client/src/diagnostics.ts — 日志通道       │
│  ├── server/                   — 预构建二进制 │
│  └── syntaxes/p4.tmLanguage.json — 语法高亮   │
└────────────────┬──────────────────────────────┘
                 │ stdio / socket
                 ▼
┌─────────────────────────────────────────────┐
│  p4lsp-server (Rust LSP Server)              │
│  ┌─────────┐  ┌─────────┐  ┌──────────────┐ │
│  │ server  │  │document │  │ workspace    │ │
│  │ (主入口)│  │ (Rope)  │  │ (全局索引)   │ │
│  └────┬────┘  └────┬────┘  └──────┬───────┘ │
│       │            │               │         │
│  ┌────▼────┐ ┌─────▼────┐ ┌───────▼──────┐ │
│  │ hover   │ │completion│ │ diagnostics  │ │
│  │ (悬停)  │ │ (补全)   │ │ (诊断)       │ │
│  └────┬────┘ └─────┬────┘ └───────┬──────┘ │
│  ┌────▼────┐ ┌─────▼────┐ ┌───────▼──────┐ │
│  │ index   │ │typer     │ │ typecheck    │ │
│  │ (大纲)  │ │ (类型)   │ │ (语义检查)   │ │
│  └────┬────┘ └─────┬────┘ └──────────────┘ │
│  ┌────▼────┐ ┌─────▼────┐ ┌──────────────┐ │
│  │semantic │ │rename    │ │ references   │ │
│  │ tokens  │ │ (重命名) │ │ (引用查找)   │ │
│  └─────────┘ └──────────┘ └──────────────┘ │
└─────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────┐
│  tree-sitter-p4 (C Grammar + Rust FFI)     │
│  ├── grammar.js              — P4-16 语法定义│
│  └── src/parser.c            — 生成解析器    │
└─────────────────────────────────────────────┘
```

## 核心数据流

### 1. 文档生命周期 (TextDocumentSync)

```
did_open ──► Document::new(text, parser)
              ├── Rope::from_str(text)     — 增量编辑用的 rope 数据结构
              ├── parser.parse(text)       — Tree-sitter AST
              └── publish_diagnostics()    — 即时诊断
did_change ──► apply_changes(changes)
                ├── Rope 增量更新
                ├── reparse()              — Tree-sitter 增量重解析
                └── index_document()       — 更新全局索引
did_close ──► remove_document() — 清理内存
```

### 2. 语言特性请求链路

| 请求 | 入口 | 数据依赖 |
|------|------|---------|
| `hover` | `hover.rs::hover_with_workspace` | `workspace_index.scope_at` → 局部变量 → `typer.rs` → 全局定义 |
| `completion` | `completion.rs::completions` | `scope_at` + `infer_expr_type` + 内置库 |
| `definition` | `server.rs::resolve_definition` | `scope_at` → 局部定义 / `resolve_symbol` → 全局索引 |
| `references` | `references.rs::collect_reference_nodes` | 当前文件 AST 文本搜索 |
| `documentSymbol` | `index.rs::document_symbols` | 当前 AST 的声明节点遍历 |
| `semanticTokens` | `semantic_tokens.rs` | AST 节点 kind → token type 映射 |
| `rename` | `rename.rs::prepare_rename` + `build_workspace_edit` | 引用节点收集 + WorkspaceEdit |
| `signatureHelp` | `server.rs` | call 节点识别 → `hover.rs::find_method_signature` |
| `diagnostics` | `diagnostics.rs` | `tree_diagnostics` + `semantic_diagnostics` |

### 3. 全局索引构建

```
initialize(workspace_folders)
  └─► tokio::spawn 后台扫描
       ├── collect_p4_files(dir) — 递归收集 .p4
       ├── 逐个解析为 Document
       └── workspace_index.index_document(uri, tree, text)
            ├── 提取所有声明（header/struct/enum/parser/control/action/table/extern）
            ├── 提取局部作用域（action 参数、局部变量）
            └── 存入 DashMap<name, Vec<(uri, Symbol)>>
```

## 模块职责

| 模块 | 文件 | 职责 |
|------|------|------|
| `server` | `server.rs` | LSP 协议主入口、能力注册、请求分发、文档生命周期管理 |
| `document` | `document.rs` | Rope 增量文本编辑、Tree-sitter 增量重解析 |
| `parser` | `parser.rs` | Tree-sitter 解析器初始化（grammar 加载） |
| `workspace` | `workspace.rs` | 全局符号索引（DashMap）、作用域查询（scope_at）、跨文件符号解析 |
| `hover` | `hover.rs` | Hover 信息生成：定义摘要、方法签名、字段类型 |
| `completion` | `completion.rs` | CompletionItem 生成：局部符号、类型字段、内置库 |
| `diagnostics` | `diagnostics.rs` | 语法错误（tree-sitter）+ 语义诊断（未定义引用、重复定义） |
| `typer` | `typer.rs` | 表达式类型推断：`infer_expr_type`、字段类型查找 |
| `typecheck` | `typecheck.rs` | 语义检查入口（当前 stub，预留） |
| `index` | `index.rs` | Document Symbol（Outline 视图） |
| `references` | `references.rs` | 单文件引用位置收集 |
| `rename` | `rename.rs` | Rename Provider：引用收集 + WorkspaceEdit 构建 |
| `semantic_tokens` | `semantic_tokens.rs` | Semantic Tokens（Token type/modifier 映射） |

## 并发模型

- **主线程**: `tower-lsp` tokio runtime，处理所有 LSP 请求
- **后台线程**: `initialize` 时 `tokio::spawn` 预扫描 workspace 文件
- **并发安全**: `DashMap`（无锁并发哈希）用于全局索引、`std::sync::Mutex` 保护 workspace_folders
- **解析**: Tree-sitter 解析器每个文件独立，无全局状态竞争

## 关键设计决策

1. **不用 p4c**：自研 AST 语义推导，零外部进程依赖，避免 p4c 编译时延迟
2. **Tree-sitter 替代手写词法分析器**：原生增量解析 + 错误恢复，IDE 场景低延迟
3. **统一引擎**：CLI (`cargo run --example check_errors`) 与 LSP Server 共享分析代码
4. **Rust 全栈**：内存安全 + 零成本抽象，高频编辑场景无 GC 停顿

---

*最后更新: 2026-05-11*
