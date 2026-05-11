# P4LSP 开发路线图

> 按优先级排序，开发完成后及时更新 STATUS.md 和此文档

---

## Phase 1: 补全基础能力（进行中）

| 任务 | 优先级 | 目标 | 阻塞项 |
|------|--------|------|--------|
| **Workspace Symbol** | P1 | 全局符号搜索（Ctrl+T） | 接入 `workspace_index.resolve_symbol` |
| **Find References 跨文件** | P1 | 从单文件扩展到全局索引 | workspace 索引需支持反向引用链 |
| **Rename 局部变量完善** | P1 | apply 块内局部变量重命名 | 局部作用域遍历精度 |
| **Semantic Tokens 修复** | P1 | 消除 token 重叠/长度警告 | token 映射规则校对 |
| **Signature Help 完善** | P1 | extern method 签名正确返回 | hover.rs 签名库扩展 |
| **#include 配置链路** | P1 | `includePaths` 从 VS Code 配置下发到 server | workspace/configuration 请求 |

## Phase 2: 语义深度（中期）

| 任务 | 优先级 | 目标 |
|------|--------|------|
| **类型检查** | P2 | 赋值兼容性、表达式类型推导、函数调用参数匹配 |
| **常量传播/折叠** | P2 | `const` 求值、table size 常量检查、宽度参数检查 |
| **数组边界检查** | P2 | `bit<x>` 宽度合法性、数组越界 |
| **header 有效性检查** | P2 | `.isValid()` bool 上下文约束 |
| **作用域穿透检查** | P2 | action 引用 control local 合法性 |

## Phase 3: IDE 体验优化（中期）

| 任务 | 优先级 | 目标 |
|------|--------|------|
| **Code Action** | P3 | 快速修复：未定义引用 → 自动导入/创建声明 |
| **Code Lens** | P3 | table entries 计数、action 引用数 |
| **Inlay Hints** | P3 | 参数名提示、类型推断提示 |
| **Format/Range Format** | P3 | P4 代码格式化 |
| **大文件性能** | P3 | >1000 行 P4 文件编辑延迟 <100ms |
| **Unused Variable Warning** | P3 | 未使用变量/参数诊断 |

## Phase 4: 多客户端 + 生产化（远期）

| 任务 | 优先级 | 目标 |
|------|--------|------|
| **Neovim/Vim 客户端** | P4 | nvim-lspconfig 适配 |
| **Emacs 客户端** | P4 | lsp-mode / eglot 适配 |
| **CLI 增强** | P4 | `p4lsp check` 支持批量检查、输出格式选项 |
| **CI/CD** | P4 | 自动化测试、.vsix 自动发布 |
| **崩溃报告** | P4 | server 崩溃自动收集日志并上报 |

---

## 已完成里程碑

| 日期 | 里程碑 |
|------|--------|
| 2026-05-08 | 项目重新立项，从零开始 Rust LSP Server |
| 2026-05-09 | Hover + Completion + Goto Definition 基础实现 |
| 2026-05-10 | 74 tests passing；工作区预扫描、嵌套字段解析、UTF-16 修正、#include 基础解析全部完成 |
| 2026-05-11 | docs 目录重构：ARCHITECTURE / STATUS / ROADMAP 上线 |

---

*最后更新: 2026-05-11*
