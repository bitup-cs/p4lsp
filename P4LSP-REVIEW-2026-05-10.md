# P4 LSP 深度 Review 报告

> Review 日期: 2026-05-10
> 维度: 多文件搜索路径 / 嵌套结构 / 客户端-LSP 服务端交互

---

## 一、多文件搜索路径

### 🔴 严重缺陷

| # | 问题 | 影响 | 位置 |
|---|------|------|------|
| 1.1 | **`#include` 完全未解析** | 多文件项目（所有真实P4程序）不可用 | `diagnostics.rs` 仅跳过 `#` 错误，不解析路径 |
| 1.2 | **工作区文件未预扫描** | 只索引用户打开的文件，其他 `.p4` 不可见 | `server.rs::initialize` 未扫描 workspace |
| 1.3 | **`includePaths` 配置未使用** | package.json 声明了，server 端完全没读 | 客户端→服务端配置链路断裂 |
| 1.4 | **跨文件类型解析缺失** | `dot_completions` 只搜当前 AST，类型定义在另一文件时补全为空 | `completion.rs::type_completions` |
| 1.5 | **同名符号无优先级** | `resolve_symbol` 返回所有匹配，不区分定义顺序/可见性 | `workspace.rs::resolve_symbol` |

### 代码证据

```rust
// diagnostics.rs — 只跳过，不解析
tree_diagnostics() {
    if text.trim_start().starts_with('#') { return; }  // ← 直接忽略
}

// server.rs::initialize — 完全没扫描 workspace files
async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
    // InitializeParams 里有 workspace_folders，被丢弃了
    Ok(...)
}

// completion.rs::type_completions — 只搜当前 tree
fn type_completions(type_name, tree, source, _workspace) {
    let root = tree.root_node();  // ← 当前文件 AST 而已
    // 没有 workspace.files 跨文件查找
}
```

### 🟡 建议改进

- 在 `initialize` 中扫描 `workspace_folders` 下所有 `.p4` 文件并预索引
- 实现 `#include` 路径解析（参考 `p4c` 的 `-I` 机制）
- `includePaths` 通过 `InitializeParams.initializationOptions` 或 `workspace/configuration` 下发到 server
- `resolve_symbol` 增加距离/定义顺序优先级

---

## 二、嵌套结构

### 🔴 严重缺陷

| # | 问题 | 影响 | 位置 |
|---|------|------|------|
| 2.1 | **增量同步的 UTF-8/字符偏移错误** | 含中文注释的文件增量更新后 AST 损坏 | `document.rs::apply_changes` |
| 2.2 | **嵌套作用域 shadowing 未处理** | 同名变量全部列出，不遵循 shadowing 规则 | `workspace.rs::scope_at` |
| 2.3 | **条件块内变量可见性错误** | `collect_conditional_locals` 对 `stmt` 递归导致穿透 nested conditional | `workspace.rs` |
| 2.4 | **嵌套字段类型解析不完整** | `obj.b.` 需要全局 workspace 查找 `B` 的定义，但只搜当前 tree | `completion.rs::infer_expr_type` |
| 2.5 | **annotated 结构处理代码重复** | workspace.rs 和 index.rs 各实现一套 annotated_action/table 解析 | 维护风险 |

### 代码证据

```rust
// document.rs — char count 当 byte count 用，中文会炸
fn apply_changes(&mut self, changes) {
    let new_end_byte = start_byte + change.text.len(); // ← OK: byte
    ...
    let new_end_position = ts_end_point(
        &self.rope,
        start_idx + change.text.chars().count()  // ← BUG: char count!
    );
}

// workspace.rs::scope_at — 全部收集，无 shadowing
pub fn scope_at(&self, ...) -> Scope {
    let mut scope = Scope::default();
    while let Some(n) = current {
        match n.kind() {
            "action" => collect_action_params(n, source, &mut scope),  // ← 同名全推入
            ...
        }
    }
    scope  // 同名变量全在里面，最内层没有覆盖外层
}

// workspace.rs::collect_conditional_locals — 递归穿透问题
fn collect_conditional_locals(node, source, scope) {
    match child.kind() {
        "stmt" => collect_conditional_locals(child, source, scope),  // ← 会穿透 nested if
        "conditional" => {}  // ← 声称不递归，但上面的 stmt 会把它包进去递归
    }
}
```

### 🟡 建议改进

- `document.rs`: `new_end_position` 用 `rope.char_to_byte` 计算正确 byte offset
- `scope_at`: 引入 shadowing — 同名符号先查内层，查到就不继续收集外层
- `collect_conditional_locals`: 用 AST 层级严格隔离，不穿透 `conditional` 边界
- `infer_expr_type`: 传入 `workspace` 做跨文件类型查找
- annotated 解析抽成公共函数（`workspace.rs` 和 `index.rs` 共用）

---

## 三、客户端与 LSP 服务端交互

### 🔴 严重缺陷

| # | 问题 | 影响 | 位置 |
|---|------|------|------|
| 3.1 | **多处 `.expect()` 导致 server 崩溃** | grammar 加载失败/解析失败时整个进程退出 | `parser.rs`, `document.rs`, `server.rs` |
| 3.2 | **客户端无 server 启动错误处理** | server 二进制不存在时静默失败 | `extension.ts` |
| 3.3 | **无自动重连/重启机制** | server 崩溃后插件完全失效 | `extension.ts` |
| 3.4 | **不处理 `untitled` scheme** | 新建未保存 `.p4` 文件 LSP 不生效 | `extension.ts::documentSelector` |
| 3.5 | **`shutdown` 不清理资源** | 长时间运行后内存泄漏 | `server.rs::shutdown` |
| 3.6 | **`did_change` 中阻塞式索引** | 大文件编辑时 LSP 响应卡顿 | `server.rs::did_change` |

### 代码证据

```rust
// parser.rs — grammar 加载失败直接 panic
parser.set_language(&language()).expect("set language");

// document.rs — 全量替换时直接 panic
tree = parser.parse(&change.text, None).expect("parse");

// server.rs — did_open/did_change 都 expect
tree = parser.parse(&text, None).expect("parse");
entry.reparse();  // ← 内部也是 expect("reparse")

// extension.ts — 无错误处理
client = new LanguageClient(..., serverOptions, clientOptions);
client.start();  // ← 没有 try/catch，没有 onReady()

// extension.ts — 不处理 untitled
documentSelector: [{ scheme: "file", language: "p4" }]
// 缺少 { scheme: "untitled", language: "p4" }

// server.rs::shutdown — 空实现
async fn shutdown(&self) -> Result<()> {
    Ok(())  // ← documents 和 workspace_index 都没清理
}
```

### 🟡 建议改进

- 所有 `.expect()` 改为 `Result` 传播，客户端 graceful degradation
- `extension.ts` 添加：
  - `client.start()` 的 `try/catch`
  - `client.onDidChangeState` 监听断开
  - 自动重启逻辑（指数退避）
  - `OutputChannel` 用于用户可见的诊断输出
- `shutdown` 中清理 `documents.clear()` + `workspace_index` 清理
- `did_change` 考虑把索引放到后台线程（`tokio::spawn`）
- `documentSelector` 增加 `untitled` scheme

---

## 四、测试覆盖盲区

| 盲区 | 当前状态 | 风险 |
|------|----------|------|
| 多文件交叉引用补全 | 无测试 | 跨文件场景回归无保障 |
| `#include` 解析 | 无测试 | 核心功能未实现 |
| 含 UTF-8 字符的增量同步 | 无测试 | 中文注释场景必炸 |
| Server 崩溃恢复 | 无测试 | 生产环境稳定性未知 |
| 大文件性能（>1000行） | 无测试 | 实际项目规模未知 |

---

## 五、优先级排序

**P0（本周必须修）：**
1. 3.1 所有 `.expect()` → `Result`（server 不能 crash）
2. 2.1 `document.rs` 增量同步 char→byte 修复
3. 3.4 `untitled` scheme 支持

**P1（下周）：**
4. 1.1 `#include` 基础解析
5. 3.2 客户端 server 启动错误处理
6. 2.2 作用域 shadowing

**P2（后续）：**
7. 1.2 工作区预扫描
8. 3.3 自动重连
9. 1.4 跨文件类型解析

---

*Reviewer: 打工人 🔧*
