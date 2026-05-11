# P4LSP 功能状态

> 实时反映 p4lsp-server + p4-vscode 各功能的实现状态。开发完成后请同步更新此文档。

---

## LSP 服务端功能 (p4lsp-server)

| 功能 | 状态 | 说明 |
|------|------|------|
| **TextDocumentSync** | ✅ | 增量同步 + 全量同步，UTF-16/UTF-8 偏移修正完成 |
| **Hover** | ✅ | header/struct/enum/parser/control/action/table/extern 定义摘要；method call 签名；field access 类型；局部变量/参数类型 |
| **Goto Definition** | ✅ | 单文件内跳转 + 全局索引跨文件解析 |
| **Goto Declaration** | ✅ | 与 Goto Definition 共享 `resolve_definition` |
| **Completion** | ✅ | 局部作用域符号、类型推导字段/方法、P4 内置库（`isValid`/`setInvalid`、`extract`、`emit` 等） |
| **Document Symbols** | ✅ | 当前文件大纲（Outline 视图） |
| **Diagnostics** | ✅ | 语法错误（tree-sitter）+ 语义诊断（未定义引用检测、重复定义检测） |
| **Workspace Symbol** | ✅ | 全局符号搜索（Ctrl+T），遍历所有已索引文件 |
| **Find References** | ✅ | 单文件 + 跨文件全局索引遍历 |
| **Rename** | ✅ | 基础实现 + apply 块局部变量 + action 边界隔离 + 跨文件 |
| **Signature Help** | ⚠️ | handler 已实现，call 节点识别 + 参数索引计算完成；extern method 签名接入待完善 |
| **Semantic Tokens** | ⚠️ | 基本实现完成；token 重叠/长度警告待修复 |
| **Code Action** | ❌ | 未实现 |
| **Code Lens** | ❌ | 未实现 |
| **Inlay Hints** | ❌ | 未实现 |
| **Format/Range Format** | ❌ | 未实现 |

## VS Code 客户端功能 (p4-vscode)

| 功能 | 状态 | 说明 |
|------|------|------|
| **语法高亮** | ✅ | TextMate (`p4.tmLanguage.json`) |
| **服务器启动** | ✅ | bundled → PATH → 常见路径自动查找 |
| **日志通道** | ✅ | OutputChannel + 请求计时 |
| **untitled scheme** | ✅ | 新建未保存 `.p4` 文件支持 |
| **自动重连** | ✅ | 客户端指数退避重连 |
| **错误处理** | ✅ | server 启动失败优雅降级 |
| **.vsix 打包** | ✅ | 修复完毕 |
| **Snippets** | ✅ | 基础代码片段 |
| **Middleware 日志** | ✅ | 每个 LSP 请求计时 |

## 基础设施

| 功能 | 状态 | 说明 |
|------|------|------|
| **Tree-sitter 解析** | ✅ | P4-16 语法，增量解析，错误恢复 |
| **Grammar 修复** | ✅ | for/verify/continue/break/compound assignment 已补全 |
| **工作区预扫描** | ✅ | initialize 时递归扫描 workspace_folders 下所有 `.p4` 并索引 |
| **文档增量同步** | ✅ | Rope + UTF-16 偏移修正 |
| **全局索引 (DashMap)** | ✅ | 声明提取 + 作用域收集 |
| **作用域查询 (scope_at)** | ✅ | action 参数、局部变量、for 循环变量、shadowing 支持 |
| **类型推导 (typer)** | ✅ | 表达式类型推断、嵌套字段类型解析 |
| **#include 解析** | ✅ | 配置链路完整（客户端下发 → 服务端接收存储） |
| **跨文件类型解析** | ⚠️ | 预索引完成后大部分场景可用；复杂嵌套场景待增强 |
| **常量传播/折叠** | ❌ | 未实现 |
| **数组边界检查** | ❌ | 未实现 |
| **未使用变量检测** | ❌ | 未实现 |
| **Neovim/Vim 客户端** | ❌ | Phase 4 规划 |

## 测试覆盖

| 测试类型 | 状态 | 数量 |
|----------|------|------|
| 单元测试 | ✅ | 74 passed, 0 failed |
| 示例/CLI | ✅ | `check_errors`, `check_symbols`, `parse_file` 可用 |
| VS Code E2E | ⚠️ | 基础框架就绪，待扩展用例 |

---

**图例**: ✅ 已完成 | ⚠️ 部分实现 / 有已知问题 | ⏸️ stub/占位 | ❌ 未开始

*最后更新: 2026-05-11 | 测试基线: cargo test — 74 passed, 0 failed*
