# P4LSP — P4 语言服务器

[![CI](https://github.com/bitup-cs/p4lsp/actions/workflows/test.yml/badge.svg)](https://github.com/bitup-cs/p4lsp/actions)

> 基于 Tree-sitter + Rust 的 P4-16 语言服务器协议（LSP）实现，配套 VS Code 插件。

## 项目结构

```
p4lsp/
├── p4lsp-server/          # Rust LSP Server（核心引擎）
│   ├── src/
│   │   ├── server.rs        # LSP 主入口
│   │   ├── parser.rs        # Tree-sitter 解析
│   │   ├── workspace.rs     # 工作区索引 / #include 解析
│   │   ├── typer.rs         # 类型推导系统
│   │   ├── hover.rs         # Hover Provider
│   │   ├── completion.rs    # Completion Provider
│   │   ├── diagnostics.rs   # 诊断发布
│   │   ├── semantic_tokens.rs # Semantic Tokens
│   │   ├── rename.rs        # Rename Provider
│   │   ├── index.rs         # Document Symbols
│   │   └── ...
│   └── examples/            # CLI 工具与调试脚本
├── p4-vscode/               # VS Code 插件客户端
│   ├── client/              # TypeScript 客户端代码
│   ├── server/              # 预构建的 LSP 二进制
│   └── syntaxes/            # TextMate 语法高亮
├── tree-sitter-p4/          # P4-16 Tree-sitter Grammar
│   ├── grammar.js           # 语法定义
│   └── src/parser.c         # 生成解析器
├── stdlib/                  # P4 标准库定义
│   ├── builtins.json        # 内置函数签名
│   ├── core.p4              # 核心类型
│   └── v1model.p4           # v1model 架构定义
└── docs/                    # 设计文档
```

## 功能状态

| 功能 | 状态 | 说明 |
|------|------|------|
| 语法高亮 | ✅ | TextMate + Tree-sitter 双重支持 |
| Hover | ✅ | header/struct/enum/parser/control/action/table/extern |
| Goto Definition | ✅ | 单文件内符号跳转 |
| Completion | ✅ | 局部作用域 + 类型推导字段方法 |
| Diagnostics | ✅ | 语法错误 + 未定义引用 |
| Document Symbols | ✅ | 当前文件大纲 |
| Rename | ⏸️ | 基础实现，apply 块内局部变量待完善 |
| Signature Help | ⏸️ | 基础设施就绪，extern method 待接入 |
| Find References | ⏸️ | 跨文件索引待完善 |
| Semantic Tokens | ⏸️ | 基本实现，token 重叠/长度警告待修复 |
| `#include` 解析 | ⏸️ | 配置已声明，服务端未实现 |
| 跨文件类型解析 | ⏸️ | 仅索引打开的文件 |

完整差距分析见 [docs/gap-analysis.md](docs/gap-analysis.md)。

## 快速开始

### 1. 构建服务器

```bash
cd p4lsp-server
cargo build --release
```

### 2. 安装 VS Code 插件

```bash
cd p4-vscode
npm install
npm run compile
# 按 F5 启动 Extension Host 调试
```

或使用预构建包：

```bash
code --install-extension p4-vscode-*.vsix
```

### 3. CLI 检查

```bash
cd p4lsp-server
cargo run --example check_errors -- /path/to/program.p4
cargo run --example check_symbols -- /path/to/program.p4
```

## 测试

### 服务器端单元测试

```bash
cd p4lsp-server
cargo test
```

### E2E 测试（VS Code 插件）

```bash
cd p4-vscode
npm run test:headless   # Xvfb 无头环境
```

当前状态：8 passing / 5 pending / 0 failing。

## 技术栈

- **解析**: [Tree-sitter](https://tree-sitter.github.io/tree-sitter/)（增量解析 + 错误恢复）
- **LSP 框架**: [tower-lsp](https://github.com/ebkalderon/tower-lsp)（tokio 异步）
- **并发**: [rayon](https://github.com/rayon-rs/rayon)（并行索引）
- **符号表**: [dashmap](https://github.com/xacrimon/dashmap)（并发哈希）
- **客户端**: VS Code Extension API + vscode-languageclient

## 设计决策

1. **不用 p4c**: 自研 AST 语义推导，零外部进程依赖
2. **Tree-sitter 替代手写词法分析器**: 原生增量解析，IDE 场景低延迟
3. **统一引擎**: CLI (`p4lsp check`) 与 LSP Server 共享分析代码
4. **Rust 全栈**: 内存安全 + 零成本抽象，适合高频编辑场景

## 文档

| 文档 | 内容 |
|------|------|
| [docs/gap-analysis.md](docs/gap-analysis.md) | P4-16 spec v1.2.5 vs 当前实现的完整差距分析 |
| [docs/review-2026-05-10.md](docs/review-2026-05-10.md) | 多文件搜索路径 / 嵌套结构 / 客户端交互深度评审 |
| [docs/vscode-redesign-plan.md](docs/vscode-redesign-plan.md) | VS Code 插件三阶段重构计划 |
| [docs/tree-sitter-audit.md](docs/tree-sitter-audit.md) | Tree-sitter P4 Grammar 审计与修复记录 |
| [p4-vscode/CHANGELOG.md](p4-vscode/CHANGELOG.md) | 版本变更记录 |
| [p4-vscode/README.md](p4-vscode/README.md) | VS Code 插件详细说明 |

## 路线图

| 阶段 | 目标 | 时间 |
|------|------|------|
| P0 | 语法修复（for/verify/continue/break/compound assignment） | 近期 |
| P1 | `#include` 解析 + 工作区预扫描 | 近期 |
| P2 | 跨文件类型解析 + 全局符号索引 | 中期 |
| P3 | 深度语义（常量传播、数组边界、性能优化） | 中期 |
| P4 | Neovim/Vim 客户端 + 生产化 | 远期 |

## License

MIT
