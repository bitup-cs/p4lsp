# tree-sitter-p4 Grammar 完整性审计报告

**审计日期**: 2026-05-08  
**审计对象**: `vendor/tree-sitter-p4/grammar.js`  
**来源仓库**: https://github.com/oxidecomputer/tree-sitter-p4.git  
**Commit**: `ec27f51`（`string literals do exist in p4, also more binops (#4)`）  
**对照标准**: P4-16 Language Specification v1.2.5  
**测试方法**: 静态 grammar.js 分析 + tree-sitter parse 验证（29 个示例文件 + 15 个边界测试）

---

## 一、概述

该 grammar 是一个**早期实验性项目**，代码量约 13KB，只有一个 commit 历史。它能解析作者编写的 29 个示例文件（这些文件刻意避开了不支持的语法），但在面对**标准 P4-16 语法时存在大量缺失和错误**。

**关键结论**：此 grammar **不适合直接用于生产级 P4 LSP Server**。核心语法覆盖率不足，存在大量解析歧义和缺失节点，需要大规模重写或另寻替代方案。

---

## 二、Critical 缺陷（完全缺失，导致解析失败）

| # | 缺失语法 | P4-16 Spec 章节 | 影响 | 测试结果 |
|---|---------|----------------|------|---------|
| 1 | `header_union` 声明 | 7.2.3 | 无法解析 header_union 类型 | ❌ ERROR |
| 2 | `enum` 声明 | 7.2.1 | 无法解析 enum 类型 | ❌ ERROR |
| 3 | `error { ... }` 声明 | 7.1.2 | 无法解析 error 枚举声明 | ❌ ERROR |
| 4 | `match_kind { ... }` 声明 | 7.1.3 | 无法解析 match_kind 声明 | ❌ ERROR |
| 5 | `switch` 语句 | 12.7 | 无法解析 control apply 中的 switch | ❌ ERROR |
| 6 | `exit` 语句 | 12.5 | 无法解析 exit 控制流 | ❌ ERROR |
| 7 | `return expr;` | 12.4 | 只有 `return;`，缺少带值返回 | ❌ ERROR |
| 8 | `abstract` extern 方法 | 7.2.5 | 无法解析 abstract method 声明 | ❌ ERROR |
| 9 | `list` 表达式 `(e1, e2)` | 8.14 | table entries 中的 tuple key 无法解析 | ❌ MISSING/ERROR |
| 10 | `cast` 表达式 `(Type)expr` | 8.10 | 在一般表达式位置（如 if 条件）无法解析 | ❌ ERROR |
| 11 | `header stack` `H[N]` | 7.2.4 | 数组索引在类型声明中失败 | ❌ ERROR |
| 12 | `this` 关键字 | 7.2.5 | extern 方法中的 this 引用缺失 | ❌ 未定义 |

### 详细说明

**1. `header_union` 缺失**
```p4
header_union U { H1 h1; H2 h2; }  // ❌ 完全无法解析
```
grammar.js 的 `_definition` 中没有 `header_union_definition` 规则。

**2. `enum` 缺失**
```p4
enum Color { Red, Green, Blue }   // ❌ ERROR
```
grammar 只有 `_type` 中引用了 `error` 作为类型名，但没有 `error { ... }` 和 `enum { ... }` 的声明规则。

**5. `switch` 语句缺失**
```p4
switch (t.apply().action_run) {   // ❌ ERROR
    a1: { }
    default: { }
}
```
`stmt` 中没有 `switch` 分支。

**7. `return expr;` 缺失**
```p4
bit<32> f() { return 1; }        // ❌ ERROR
```
grammar 中只有 `seq("return", ";")`，没有 `seq("return", $.expr, ";")`。

**9. `list` 表达式 `(e1, e2)` 缺失 — 严重影响 table entries 解析**
```p4
const entries = {
    (0x01, 0x1111 &&& 0xF) : a_with_control_params(1);  // ❌ 解析混乱
}
```
grammar 中 `tuple` 定义为 `{ expr, expr }`（花括号），但 P4 中 list 表达式用 `(e1, e2)`（圆括号）。table entries 的 key 被错误解析为嵌套 `call` 节点，产生空 `identifier` 和 `MISSING` 节点。

**10. `cast` 表达式缺失 — 影响语义分析**
```p4
if ((bit<32>)x > 0) { }          // ❌ ERROR
```
cast 只能在 `var_choice`（变量初始化右侧）中以 `(type) expr` 形式出现，不能在一般 `expr` 中作为 cast 节点识别。这导致所有 cast 表达式在 if 条件、函数参数、赋值 RHS 等位置失败。

**11. `header stack` `H[N]` 缺失**
```p4
struct headers { H[4] stack; }    // ❌ ERROR（`[4]` 处产生 ERROR 节点）
```
`field` 规则只支持 `type identifier ";"`，不支持 `H[4] stack;` 这种带数组下标的字段声明。

---

## 三、Warning 级别问题（能解析但结果不准确/有歧义）

| # | 问题 | 说明 | 影响 |
|---|------|------|------|
| 1 | `tuple` 用 `{ ... }` 而非 `( ... )` | grammar 定义 `tuple = "{" expr ... "}"`，但 P4-16 标准 list 表达式是 `(e1, e2)` | 与标准 P4 不一致；list.p4 示例用 `{ ... }` 可能依赖特定架构扩展 |
| 2 | `expr` 二元运算 `optional($.expr)` | `prec.left(2, seq(optional($.expr), $.binop, $.expr))` 允许 `+ b` 这种前缀表达式 | 解析歧义，tree-sitter 错误恢复时可能产生诡异 AST |
| 3 | `binop` 混入一元运算符 | `!`（逻辑非）、`=`（赋值）与 `==` 放在同一个 binop 规则中 | `!x` 被当作二元运算处理；`=` 和 `==` 在表达式中无区分 |
| 4 | `state` 规则含怪异形式 | `seq($.method_identifier, "()", $.type_identifier, ";")` | 不清楚这是什么语法，不在 P4-16 标准中 |
| 5 | `stmt` 混入 `$.action` | action 声明出现在 statement 选择中 | action 不是 statement，此规则错误 |
| 6 | `package` 第二种形式怪异 | `seq($.method_identifier, "(", choice(seq($.method_identifier, "(", ")"), $.identifier), ...)` | 不符合 P4 标准 package 语法 |
| 7 | `table_element` `const entries` 解析混乱 | 如 Critical #9 所述，entries 内容被拆成多个不连贯的 `expr` + `action_item` | 无法从 AST 重建 entries 结构 |
| 8 | `accept`/`reject` 当作普通 identifier | `transition accept;` 解析为 `transition -> identifier("accept")` | 没有特殊节点标记这些 parser 终止状态 |
| 9 | `parameter` 逗号位置混乱 | 多个 choice 分支中 `optional(",")` 位置不一致 | 可能导致 `f(a, b,)` 这类无效语法被接受 |
| 10 | `annotation_content` 过宽 | 允许 `$.expr` 作为 annotation 参数 | P4 annotation 参数通常是字符串或名字 |
| 11 | `slice` 缺少单索引 | 只支持 `[number : number]`，不支持 `stack[0]` 单索引 | header stack 访问受限 |
| 12 | `identifier_preproc` 大写限制 | `/[A-Z][A-Z0-9_]*/` | 预处理器宏不一定全大写 |
| 13 | `method` 类型参数单一 | `optional(seq("<", $.type_identifier, ">"))` 只支持单个类型参数 | extern 方法如 `extract<T>(out T hdr)` 只能有 1 个泛型参数 |
| 14 | `control_var` 覆盖不全 | 第二种形式 `choice($._type, $.type_identifier) $.identifier ";"` 允许 `Checksum csum;`（不带括号）| extern 实例化语法宽松，可能接受不合法代码 |
| 15 | `preproc` `#define` 覆盖极窄 | 只支持有限的宏定义形式（常量、struct-like 块、简单函数）| 复杂 `#define` 宏无法正确解析 |

---

## 四、IDE 支持缺失

`queries/highlights.scm` 仅覆盖基础关键字高亮，缺失：

- `header_union`、`enum`、`switch`、`exit`、`return`、`abstract`、`this` 等关键字高亮
- `locals.scm`（局部变量/作用域追踪）
- `tags.scm`（符号跳转）
- `folds.scm`（代码折叠）
- `indent.scm`（缩进规则）

---

## 五、与 P4-16 Spec 语法节点对照表

| Spec 语法节点 | grammar.js 规则 | 状态 |
|--------------|----------------|------|
| `program` / `source_file` | `source_file: repeat($.top)` | ✅ |
| `constantDeclaration` | `const_definition` | ✅ |
| `externDeclaration` | `extern_definition` | ⚠️ 缺少 abstract |
| `actionDeclaration` | `action` | ✅ |
| `parserDeclaration` | `parser_definition` | ✅ |
| `controlDeclaration` | `control_definition` | ✅ |
| `packageDeclaration` | `package` | ⚠️ 第二种形式怪异 |
| `functionDeclaration` | `function_declaration` | ⚠️ 缺少 return expr |
| `headerTypeDeclaration` | `header_definition` | ✅ |
| `headerUnionDeclaration` | — | ❌ 缺失 |
| `structTypeDeclaration` | `struct_definition` | ✅ |
| `enumDeclaration` | — | ❌ 缺失 |
| `errorDeclaration` | — | ❌ 缺失 |
| `matchKindDeclaration` | — | ❌ 缺失 |
| `typedefDeclaration` | `typedef_definition` | ✅ |
| `instantiation` | `control_var` / — | ⚠️ 顶层实例化缺失，control 内有限支持 |
| `tableDeclaration` | `table` | ⚠️ entries 解析混乱 |
| `parserState` | `state` | ⚠️ 含怪异形式 |
| `statement` | `stmt` | ❌ 缺少 switch, exit, return expr |
| `assignmentOrMethodCallStatement` | `call` + `var_decl` | ⚠️ 赋值和声明混在一起 |
| `conditionalStatement` | `conditional` | ✅ |
| `switchStatement` | — | ❌ 缺失 |
| `expression` | `expr` | ❌ 缺少 cast, list, 一元运算 |
| `castExpression` | 仅在 `var_choice` 中 | ❌ 一般位置缺失 |
| `listExpression` | `tuple`（花括号） | ❌ 圆括号 list 缺失 |
| `typeName` | `_type` | ⚠️ 缺少 list, void |
| `annotation` | `annotation` | ⚠️ content 过宽 |
| `direction` | `direction` | ✅ |
| `parameterList` | `parameter` | ⚠️ 逗号混乱 |
| `preprocessor` | `preproc` | ⚠️ 覆盖窄 |

---

## 六、结论与建议

### 6.1 现状评估

| 指标 | 结果 |
|------|------|
| 示例文件通过率（作者自带） | 29/29 (100%) |
| 标准 P4-16 语法覆盖率（关键节点） | ~55% |
| Critical 缺失数 | 12 项 |
| Warning 级别问题 | 15 项 |
| 维护活跃度 | 极低（单 commit，无上游更新） |

### 6.2 修复成本估算

若基于现有 grammar.js 修复：
- 添加缺失节点（header_union, enum, error, match_kind, switch, exit, return, abstract, cast, list, stack）—— 约 **2-3 天**
- 重构 expr / binop / parameter 消除歧义 —— 约 **1-2 天**
- 修正 table entries / tuple / var_choice 语义 —— 约 **1 天**
- 重写 highlights.scm + 添加 locals/tags —— 约 **1 天**
- 编写完整测试用例覆盖 P4-16 语法 —— 约 **2-3 天**

**总计：约 7-10 天**，工作量接近重写一个新 grammar。

### 6.3 建议方案

**方案 A：Fork 并修复（推荐）**
1. Fork `oxidecomputer/tree-sitter-p4`
2. 系统性地补全所有 Critical 缺失节点
3. 重构 expr / statement / parameter 消除歧义
4. 编写完整的 P4-16 spec 对照测试集
5. 添加 queries（highlights, locals, folds）
6. 维护为独立 repo，定期向 oxidecomputer 提 PR

**方案 B：从零重写**
1. 基于 P4-16 spec v1.2.5 的 grammar 章节
2. 用 tree-sitter CLI 从头生成新 grammar
3. 优势：无历史包袱，语法干净；劣势：前期工作量更大（约 10-14 天）

**方案 C：寻找其他现成 grammar**
- 搜索 GitHub 是否有更完善的 tree-sitter-p4 实现
- 目前 oxidecomputer 的版本似乎是搜索结果中较完整的一个，但仍远未达标

---

## 附录 A：测试脚本

用于复现审计结果的测试文件保存在 `/tmp/test_*.p4`，可直接运行：

```bash
cd vendor/tree-sitter-p4
npx tree-sitter parse /tmp/test_enum.p4
npx tree-sitter parse /tmp/test_union.p4
npx tree-sitter parse /tmp/test_switch.p4
npx tree-sitter parse /tmp/test_exit.p4
npx tree-sitter parse /tmp/test_error_decl.p4
npx tree-sitter parse /tmp/test_match_kind.p4
npx tree-sitter parse /tmp/test_list_expr.p4
npx tree-sitter parse /tmp/test_accept_reject.p4
npx tree-sitter parse /tmp/test_abstract.p4
npx tree-sitter parse /tmp/test_return_expr.p4
npx tree-sitter parse /tmp/test_cast2.p4
npx tree-sitter parse /tmp/test_stack.p4
```

## 附录 B：grammar.js 问题定位速查

| 行号范围 | 问题 |
|---------|------|
| 12-25 | `_definition` 缺失 header_union, enum, error, match_kind, instantiation |
| 85-120 | `stmt` 混入 $.action，缺失 switch, exit, return expr |
| 122-135 | `expr` 二元运算 optional($.expr) 怪异，缺少 cast, list, 一元运算 |
| 137-145 | `binop` 混入一元 ! 和赋值 = |
| 156-165 | `control_var` 覆盖不全，顶层 instantiation 缺失 |
| 168-173 | `parameter` 逗号位置混乱 |
| 175-179 | `field` 不支持 stack 类型 `H[N]` |
| 181-183 | `tuple` 使用 `{ }` 而非 `( )` |
| 185-188 | `slice` 缺少单索引 `[N]` |
| 190-195 | `method` 只支持单类型参数 |
| 197-202 | `type_argument_list` 仅支持 type/type_identifier |
| 204-209 | `state` 含怪异形式 `method_identifier () type_identifier ;` |
| 211-216 | `table_element` entries 解析混乱 |
| 226-235 | `preproc` #define 覆盖极窄 |
