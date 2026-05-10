# VSCode P4 LSP 插件 — 重新开发规划书

**版本**: v1.0  
**日期**: 2026-05-07  
**目标**: 构建生产级、高性能的 P4-16 语言服务器与 VSCode 插件

---

## 一、现状盘点（现有代码基线）

### 1.1 p4lsp-server（Rust LSP Server）

| 组件 | 状态 | 说明 |
|------|------|------|
| `tower-lsp` | ✅ | LSP 协议层完整 |
| `tree-sitter-p4` | ✅ vendored | `build.rs` + `cc` 编译 vendored grammar，能解析基础 P4 |
| `dashmap` | ✅ | 全局符号索引 |
| `rayon` | ⚠️ 声明但未深度使用 | 可并行化索引和查询 |
| `ropey` | ✅ | 增量文本更新 |
| **Hover** | ✅ | 类型定义、实例、字段访问 |
| **Goto Definition** | ✅ | 单文件内跳转 |
| **Document Symbol** | ✅ | Outline 视图 |
| **Completion** | ✅ | 类型补全 + 作用域可见符号 + 字段访问补全 |
| **Diagnostics** | ❌ | 无语法/语义错误检查 |
| **Find References** | ❌ | |
| **Rename** | ❌ | |
| **Signature Help** | ❌ | |
| **Semantic Tokens** | ❌ | |
| **Workspace Symbol** | ❌ | |
| **Code Action** | ❌ | |
| **类型推导** | ⚠️  stub | 目前只做符号收集，无真正类型系统 |
| **#include / extern 解析** | ❌ | 单文件分析 |
| **内置方法补全** | ❌ | `isValid()`, `extract()`, `apply()` 等无补全 |

### 1.2 p4-vscode（VSCode 插件）

| 组件 | 状态 | 说明 |
|------|------|------|
| `vscode-languageclient` | ✅ | 客户端框架完整 |
| **Middleware 日志** | ✅ | 每个 LSP 请求计时 + 日志 |
| **服务器路径解析** | ✅ | bundled → PATH → 常见路径 |
| **配置项** | ⚠️ | `p4.analyzer.path` 和 `p4.languageServer.path` 重复 |
| **.vsix 打包** | ❌ | `extension.js` 第 10 行报错，需要修复 |
| **语法高亮** | ✅ | `p4.tmLanguage.json` 已配置 |
| **Snippets** | ✅ | 基础代码片段 |

---

## 二、调研结果

### 2.1 VSCode LSP 常用功能（按优先级排序）

| 优先级 | 功能 | 用户感知 | 技术复杂度 | 备注 |
|--------|------|----------|------------|------|
| **P0** | Diagnostics | 红线报错 | ⭐⭐ | 语法错误由 Tree-sitter 提供；语义错误需类型系统 |
| **P0** | Completion | 自动补全 | ⭐⭐⭐ | 需作用域 + 类型推导 + 内置函数 |
| **P0** | Hover | 悬浮提示 | ⭐⭐ | 已有基础，需补充内置函数文档 |
| **P0** | Goto Definition | 跳转定义 | ⭐⭐ | 已有基础，需跨文件支持 |
| **P1** | Document Symbol | 文件大纲 | ⭐ | 已有基础 |
| **P1** | Find References | 查找引用 | ⭐⭐⭐⭐ | 需反向索引 |
| **P1** | Rename | 重命名 | ⭐⭐⭐⭐ | 需引用分析 + Workspace Edit |
| **P1** | Signature Help | 参数提示 | ⭐⭐⭐ | extern / function / action 参数 |
| **P1** | Semantic Tokens | 语义着色 | ⭐⭐⭐ | 让 `const` 变量、类型名等有不同颜色 |
| **P2** | Workspace Symbol | 工作区符号 | ⭐⭐⭐ | 需跨文件索引 |
| **P2** | Code Action | 快速修复 | ⭐⭐⭐⭐ | Auto-import missing header 等 |
| **P2** | Formatting | 格式化 | ⭐⭐⭐⭐ | P4 无官方 formatter，可基于 Tree-sitter CST |
| **P3** | Inlay Hints | 内联提示 | ⭐⭐⭐⭐ | 显示推断类型 |
| **P3** | Call Hierarchy | 调用层级 | ⭐⭐⭐⭐⭐ | P4 无传统函数调用链 |

**关键结论**：P0 + P1 共 8 项功能是"能用"的底线，必须全部实现才能称为生产级。

### 2.2 P4 语法核心要点（影响 LSP 设计）

| 语法特性 | 对 LSP 的影响 |
|----------|---------------|
| `header` / `struct` / `header_union` | 需字段索引 + 嵌套类型展开（`struct a { bit<1> a0; } struct b { a ba; }`） |
| `control` / `parser` / `action` / `function` | 块级作用域 + 参数作用域 |
| `extern` | 声明与实现分离，目标架构可能未定义 |
| `table` | `apply()` 隐式方法 + `key` / `actions` / `default_action` 语义 |
| `package` | 顶层组合单元，跨文件实例化 |
| `#include` | 需 VFS 层解析头文件路径 |
| 泛型 `extern X<T>()` | 类型参数实例化 |
| 位宽类型 `bit<32>` / `int<32>` | 类型表示需携带宽度参数 |
| `match_kind` (`exact`, `lpm`, `ternary`) | 内置枚举，需作为关键字/补全项 |
| 隐式方法 | `header.isValid()`, `header.setValid()`, `packet.extract()`, `table.apply()` |
| 内置函数 | `digest()`, `hash()`, `markToDrop()`, `random()` |
| `error` / `enum` | 特殊类型声明 |
| Annotations `@name(...)` | 元数据存储，Hover 时显示 |

**核心难点**：P4 有大量**隐式声明**（不是用户代码显式定义的），IDE 必须内置这些知识库。

### 2.3 高性能 LSP 插件架构调研

#### 架构模式对比

| 模式 | 代表项目 | 优势 | 劣势 | 适合 P4？ |
|------|----------|------|------|-----------|
| **Rust + tower-lsp + tree-sitter** | elm-lsp-rust, KCL LSP | 原生性能、增量解析、单二进制 | 开发周期长 | ✅ **推荐** |
| **TypeScript + 调用外部编译器** | 官方 p4analyzer | 快速开发 | 依赖外部进程、启动慢 | ❌ 已排除 |
| **混合架构（Tree-sitter 语法 + LSP 语义）** | Zed, Helix | 语法层本地快、语义层 server 处理 | 需两套系统 | ✅ 部分借鉴 |

#### 关键性能数据（来自调研）

- **Tree-sitter 增量解析**：编辑后重解析 < 1ms（O(n) 增量）
- **DashMap 并发索引**：无锁读，适合多线程查询
- **tower-lsp 异步处理**：天然支持 JSON-RPC 并发请求
- **ropey 增量文本**：避免全量字符串拷贝
- **benchmark 参考**：类似架构（Rust + tree-sitter + tower-lsp）比 TypeScript LSP 快 **40-60%**

#### 我们现有架构 vs 最佳实践

| 最佳实践 | 我们现状 | 差距 |
|----------|----------|------|
| 增量解析 + 增量索引 | Tree-sitter ✅ + 但重新索引时全量重建 | 需要**增量索引更新** |
| 后台索引 Workspace | 启动时无后台索引 | 需要**后台扫描 `*.p4`** |
| 请求取消（Cancellation） | 未实现 | 大文件查询需支持取消 |
| 批量请求处理 | 单请求处理 | 可用 rayon 并行化 |
| 内存映射大文件 | 全量读入 | P4 文件通常不大，可接受 |

---

## 三、重新开发规划

### 3.1 整体架构（保持现有，局部优化）

```
┌─────────────────────────────────────────────┐
│           VSCode Extension (TypeScript)      │
│  ┌─────────────┐  ┌──────────────────────┐  │
│  │ 语法高亮    │  │ LSP Client          │  │
│  │ (tree-sitter│  │ (vscode-languageclient│  │
│  │  grammar)    │  │  + middleware 日志)  │  │
│  └─────────────┘  └──────────┬───────────┘  │
└──────────────────────────────┼──────────────┘
                               │ stdio / TCP
┌──────────────────────────────▼──────────────┐
│          P4LSP Server (Rust)                 │
│  ┌────────────────────────────────────────┐  │
│  │  tower-lsp (LSP Protocol Handler)      │  │
│  └────────────────┬───────────────────────┘  │
│                   │                          │
│  ┌────────────────▼───────────────────────┐  │
│  │  Request Router / Cancellation         │  │
│  └────────────────┬───────────────────────┘  │
│                   │                          │
│  ┌────────────────▼───────────────────────┐  │
│  │  IDE Layer (Hover/Completion/...)    │  │
│  │  - 请求分发到各 Provider              │  │
│  │  - 性能计时 + 日志                    │  │
│  └────────────────┬───────────────────────┘  │
│                   │                          │
│  ┌────────────────▼───────────────────────┐  │
│  │  Semantic Engine (核心，新增)            │  │
│  │  - 符号表 (SymbolTable)                 │  │
│  │  - 类型推导 (Type Inference)            │  │
│  │  - 作用域解析 (Scope Resolution)        │  │
│  │  - 名字解析 (Name Resolution)            │  │
│  └────────────────┬───────────────────────┘  │
│                   │                          │
│  ┌────────────────▼───────────────────────┐  │
│  │  Parse Layer (tree-sitter-p4)          │  │
│  │  - 增量 CST 解析                        │  │
│  │  - CST → AST 转换                       │  │
│  │  - 语法错误提取 → Diagnostics           │  │
│  └────────────────┬───────────────────────┘  │
│                   │                          │
│  ┌────────────────▼───────────────────────┐  │
│  │  Index Layer (DashMap + rayon)         │  │
│  │  - 文档缓存 (URI → ParsedDocument)      │  │
│  │  - 全局类型索引 (类型名 → TypeDef)       │  │
│  │  - 全局实例索引 (URI+名 → Instance)      │  │
│  │  - 定义位置索引 (名 → Location[])       │  │
│  │  - 反向引用索引 (待实现)                  │  │
│  │  - 工作区文件索引 (待实现)                │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │  Built-in Knowledge Base (内置知识库)    │  │
│  │  - core.p4 (packet_in/out, verify)      │  │
│  │  - v1model.p4 (standard metadata)       │  │
│  │  - 隐式方法 (isValid/setValid/apply)     │  │
│  │  - 内置函数 (digest/hash/markToDrop)   │  │
│  │  - match_kind (exact/lpm/ternary)       │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
```

### 3.2 开发阶段（4 阶段，约 8-10 周）

#### Phase 1: 基础设施加固（第 1-2 周）

**目标**: 让现有代码从"demo 级"升级到"工程级"

| 任务 | 验收标准 |
|------|----------|
| 1. 接入 `tree-sitter-p4` crates.io 版本 | 若 vendored 版本过旧，fork 维护或寻找 crates.io 替代；`cargo build` 一键通过 |
| 2. 语法错误提取 → Diagnostics | Tree-sitter 解析时 `has_error()` → 提取错误节点位置 → 返回 `Diagnostic` |
| 3. 统一配置系统 | `p4lsp.toml` 或 `p4_project.json` 支持 `include_paths`、`target`、`arch` |
| 4. 后台索引 Workspace | 启动时扫描工作区所有 `*.p4` 文件，增量索引 |
| 5. 请求取消支持 | `tower-lsp` 的 `CancellationToken` 接入，长查询可中断 |
| 6. 性能基准测试框架 | 记录每个 LSP 请求耗时，输出到 `p4-debug.log` |

**产出**: `cargo test` 全绿 + `cargo bench` 有基线数据 + VSCode 能看到基础红线下划线

#### Phase 2: 核心语义引擎（第 3-5 周）

**目标**: 构建真正的类型推导和语义分析能力

| 任务 | 验收标准 |
|------|----------|
| 1. 类型系统 | `Type` enum 覆盖 `bit<N>`、`int<N>`、`varbit<N>`、`header`、`struct`、`enum`、`extern`、`typedef`、`error` |
| 2. 表达式类型推导 | `hdr.ipv4.dstAddr` 能推导为 `bit<32>`；`1 + 2` 推导为 `int` |
| 3. 作用域链完整实现 | `parser` → `state` → `control` → `action` → `block` → `local` 层级正确 |
| 4. Name Resolution | 跨作用域查找符号， unresolved 报错 |
| 5. #include / extern 解析 | 解析 `#include "..."`，从 `include_paths` 找文件，建立文件依赖图 |
| 6. 语义 Diagnostics | 类型不匹配、未定义标识符、重复定义、参数数量错误 |

**关键设计决策**：
- 类型推导采用**局部推导**（非 HM 统一），P4 类型系统简单，适合 ad-hoc 推导
- `extern` 未解析时标记为 warning 而非 error（目标架构可能未加载）

**产出**: 1000 行 P4 测试代码零误报零漏报（用真实 P4 程序测试）

#### Phase 3: IDE 功能补全（第 6-7 周）

**目标**: P0 + P1 功能全部实现

| 功能 | 关键实现点 |
|------|-----------|
| **Diagnostics** | 语法错误（Tree-sitter）+ 语义错误（类型系统）同步推送 |
| **Completion** | 作用域符号 + 类型成员 + 内置函数 + `match_kind` 枚举值 |
| **Hover** | 类型信息 + 文档注释 + 内置函数说明 |
| **Goto Definition** | 跨文件跳转（利用文件依赖图） |
| **Document Symbol** | 已有，优化性能（用 Tree-sitter query 替代递归遍历） |
| **Find References** | 构建反向索引 `DefId → Vec<Location>` |
| **Rename** | `PrepareRename` + `Rename` → WorkspaceEdit |
| **Signature Help** | extern / action / function 参数列表，支持 overload 选择 |
| **Semantic Tokens** | 基于 AST 节点类型输出 token 类型（类型名、函数名、变量名、关键字等） |
| **Workspace Symbol** | 全局符号索引 + 前缀模糊搜索 |

**Completion 优先级示例**：
```
hdr.ipv4.│
    → 字段补全: version, ihl, diffserv, totalLen, dstAddr, srcAddr...
    
apply { │
    → 作用域补全: ipv4_forward (table instance), drop (action), hdr (参数)
    
bit<│
    → 类型补全 + 数字提示
```

**产出**: 9 项 P0/P1 功能全部可用，通过 E2E 测试

#### Phase 4: 插件工程化（第 8-10 周）

**目标**: 从"能跑"到"能发布"

| 任务 | 验收标准 |
|------|----------|
| 1. VSCode 插件修复打包 | `.vsix` 安装后无 `extension.js:10` 错误 |
| 2. 多平台二进制分发 | GitHub Actions 构建 `linux-x64` / `darwin-arm64` / `darwin-x64` / `win32-x64` |
| 3. 插件自动下载 server | 类似 `rust-analyzer`：首次启动自动下载对应平台二进制 |
| 4. 设置面板 | 图形化配置 `include_paths`、`target`、`p4c path` |
| 5. 遥测/崩溃报告 | 可选上报性能数据和 panic 信息 |
| 6. 文档 | README + 安装指南 + 功能清单 |

---

## 四、关键技术决策

### 4.1 为什么不用 Salsa？

早期设计书提到 Salsa，但 p4lsp-server 实际未使用。**建议保持现状**：

| 方案 | 优点 | 缺点 | 结论 |
|------|------|------|------|
| 当前 DashMap + 手动增量 | 简单、可控、无复杂依赖 | 需手写缓存逻辑 | ✅ **保持** |
| Salsa 0.26 | 自动增量、查询系统 | 学习曲线陡、P4 项目规模小收益有限 | 暂缓 |

P4 项目通常 < 100 个文件，全量重索引在 100ms 内，Salsa 的边际收益不值得复杂度。

### 4.2 tree-sitter-p4 维护策略

当前 vendored 的 tree-sitter-p4 来源不明。建议：

1. **短期**：检查 vendor/tree-sitter-p4 与 [tree-sitter-grammars/tree-sitter-p4](https://github.com/tree-sitter-grammars/tree-sitter-p4) 的差异，若可用则替换为 git submodule
2. **中期**：若上游 grammar 缺失新特性（如 `header_union`、某些 annotations），fork 维护
3. **长期**：grammar 稳定后提交到 tree-sitter-grammars 组织

### 4.3 类型推导策略

P4 类型系统特点：**无泛型函数（除了 extern 模板），无继承，无隐式转换**。

```rust
pub enum Type {
    Bit(u32),           // bit<32>
    Int(u32),           // int<32>
    Varbit(u32),        // varbit<32>
    Void,
    Error,
    String,
    Bool,
    
    // 复合类型
    Header(Name, Vec<Field>),
    Struct(Name, Vec<Field>),
    HeaderUnion(Name, Vec<Field>),
    Enum(Name, Vec<Variant>),
    
    // 引用
    TypeDef(Name, Box<Type>),  // typedef
    Extern(Name, Vec<TypeParam>),
    
    // 特殊
    Unknown,            // 推导失败，用于容错
    TemplateParam(Name), // extern<T> 的 T
}
```

推导规则简单直接：
- 字面量：`1` → `int` (untyped)，`1w32` → `bit<32>`
- 字段访问：查符号表得类型定义 → 查字段类型
- 赋值：左右类型必须完全匹配（P4 无隐式转换）
- 函数调用：参数数量 + 类型完全匹配

### 4.4 内置知识库设计

内置符号不应硬编码在代码里，应作为**数据文件**加载：

```
stdlib/
├── core.p4          // packet_in/out, verify, error codes
├── v1model.p4       // standard_metadata, checksum, counter
├── tofino.p4        // Tofino-specific (optional)
└── builtins.json    // 隐式方法、内置函数签名
```

`builtins.json` 示例：
```json
{
  "methods": {
    "header": [
      {"name": "isValid", "return": "bool", "params": []},
      {"name": "setValid", "return": "void", "params": []},
      {"name": "setInvalid", "return": "void", "params": []}
    ],
    "packet_in": [
      {"name": "extract", "return": "void", "params": [{"name": "hdr", "type": "T", "out": true}], "generic": true},
      {"name": "lookahead", "return": "T", "params": [], "generic": true},
      {"name": "advance", "return": "void", "params": [{"name": "sizeInBits", "type": "bit<32>"}]}
    ]
  }
}
```

---

## 五、性能目标

| 指标 | 目标 | 测试方法 |
|------|------|----------|
| 初始索引 100 个文件 | < 2s | `cargo bench` + 真实 P4 项目 |
| 增量编辑后重新解析 | < 5ms | Middleware 日志计时 |
| Hover 响应 | < 20ms | Middleware 日志计时 |
| Completion 响应 | < 30ms | Middleware 日志计时 |
| Goto Definition 响应 | < 20ms | Middleware 日志计时 |
| 内存占用（100 文件项目） | < 200MB | `valgrind` / `top` |
| 二进制大小 | < 20MB | `strip` + `upx`（可选） |

---

## 六、风险与缓解

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| tree-sitter-p4 grammar 不完整 | 中 | 高 | 维护 fork，必要时手写补充 parser |
| P4 语义复杂度高（尤其是 table / extern） | 中 | 中 | 先覆盖 80% 常见场景，边缘场景 graceful degradation |
| 跨平台构建复杂 | 低 | 中 | GitHub Actions matrix 构建 + 发布预编译二进制 |
| VSCode 插件市场审核 | 低 | 低 | 提前准备品牌/图标/说明 |

---

## 七、下一步行动

1. **确认 Phase 1 开始**：是否需要我立即开始加固基础设施？
2. **tree-sitter-p4 审计**：检查 vendor 目录的 grammar 完整性和更新状态
3. **测试用例准备**：收集 5-10 个真实 P4 程序作为 E2E 测试基线
4. **VSCode 插件打包问题修复**：定位 `extension.js:10` 错误根因

---

*文档结束*
