# P4LSP Review 修复进度

## P0（全部完成）
- [x] 3.1 所有 `.expect()` → `Result`
- [x] 2.1 `document.rs` 增量同步 UTF-16 偏移修复
- [x] 3.4 `extension.ts` `untitled` scheme 支持 + OutputChannel + try/catch

## P1（全部完成）
- [x] `word_at_pos()` 内存优化
- [x] `HashMap → DashMap` 判定：当前设计不需要修改
- [x] `DeclarationCapability` 注册 + `goto_declaration` 实现
- [x] 2.2 作用域 shadowing 修复
- [x] 1.5 同名符号优先级（当前文件定义优先）

## P2（全部完成）
- [x] 1.2 工作区预扫描
- [x] 2.3 conditional 穿透修复
- [x] 2.5 annotated 代码重复提取
- [x] 3.3 自动重连
- [x] 2.4 嵌套字段类型解析
- [x] 1.1 `#include` 基础解析
- [x] 1.4 跨文件类型解析

## P3（全部完成）
- [x] 多文件交叉引用补全测试
- [x] 含 UTF-8 字符的增量同步测试
- [x] Server 崩溃恢复 / shutdown 资源清理
- [x] 大文件性能测试（1200 行 P4，索引 < 1 秒）

## 验证基线
- `cargo test`: **46 passed** (43 lib + 3 main)
- `cargo check --examples`: 0 errors
- `npm run compile`: 通过
