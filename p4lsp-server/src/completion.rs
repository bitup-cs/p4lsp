use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position, Url};
use tree_sitter::{Node, Tree};

use crate::workspace::{Scope, WorkspaceIndex};

/// Find the text of the node in the source.
fn node_text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

/// Find the text of a child node with the given kind.
fn child_text_by_kind(node: Node, kind: &str, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == kind {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

/// Extract the word at the given LSP position from source text.
fn word_at_pos(source: &str, pos: Position) -> Option<String> {
    let line = source.lines().nth(pos.line as usize)?;
    let col = pos.character as usize;
    let before = &line[..col.min(line.len())];
    let after = &line[col.min(line.len())..];
    
    let start = before.rfind(|c: char| !c.is_alphanumeric() && c != '_').map(|i| i + 1).unwrap_or(0);
    let end = after.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(after.len());
    let word = before[start..].to_string() + &after[..end];
    if word.is_empty() {
        None
    } else {
        Some(word)
    }
}

/// Main entry point for completions.
pub fn completions(
    uri: &Url,
    pos: Position,
    tree: &Tree,
    source: &str,
    workspace: &WorkspaceIndex,
    trigger_char: Option<&str>,
) -> Vec<CompletionItem> {
    let scope = workspace.scope_at(uri, pos, tree, source);

    // Dot-triggered completion: resolve left-hand type and return fields/methods
    if trigger_char == Some(".") {
        if let Some(dot_items) = dot_completions(pos, tree, source, &scope, workspace, uri) {
            return dot_items;
        }
        // If dot completion fails, fall through to regular completion
    }

    let mut items = Vec::new();
    let prefix = word_at_pos(source, pos);

    // 1. Local scope symbols (params + locals)
    add_scope_completions(&mut items, &scope);

    // 2. Global symbols from workspace index
    add_global_completions(&mut items, workspace, uri, prefix.as_deref());

    // 3. P4 keywords
    add_keyword_completions(&mut items);

    // 4. Standard library completions in relevant contexts
    add_stdlib_completions(&mut items, &scope, tree, source, workspace, uri);

    items
}

// ---------------------------------------------------------------------------
// Dot (`.`) completion
// ---------------------------------------------------------------------------

fn dot_completions(
    pos: Position,
    tree: &Tree,
    source: &str,
    scope: &Scope,
    workspace: &WorkspaceIndex,
    uri: &Url,
) -> Option<Vec<CompletionItem>> {
    let line_opt = source.lines().nth(pos.line as usize);
    let line = line_opt?;
    let col = pos.character as usize;
    let before_cursor = &line[..col.min(line.len())];

    // Find the last dot before cursor
    let dot_idx_opt = before_cursor.rfind('.');
    let dot_idx = dot_idx_opt?;
    let left_expr = before_cursor[..dot_idx].trim();

    if left_expr.is_empty() {
        return None;
    }

    // Infer the type of the left expression
    let ty = infer_expr_type(left_expr, scope, tree, source, workspace, uri);
    let ty = ty?;

    // Get completions for that type
    let mut items = type_completions(&ty, tree, source, workspace, uri);

    // Add built-in methods for known types
    add_builtin_methods(&ty, &mut items);

    // If the type resolves to a user-defined header, also add header built-in methods
    if is_header_type(&ty, tree, source, workspace, uri) {
        add_builtin_methods("header", &mut items);
    }

    Some(items)
}

/// Infer the type of an expression text like `hdr` or `hdr.ethernet`.
fn infer_expr_type(
    expr: &str,
    scope: &Scope,
    tree: &Tree,
    source: &str,
    workspace: &WorkspaceIndex,
    uri: &Url,
) -> Option<String> {
    let parts: Vec<&str> = expr.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    // Resolve base variable from scope
    let base = parts[0];
    let base_ty = find_var_type(base, scope);
    let mut current_ty = base_ty?;

    // Resolve each field access
    for field_name in &parts[1..] {
        let resolved = resolve_field_type(&current_ty, field_name, tree, source, workspace, uri);
        current_ty = resolved?;
    }

    Some(current_ty)
}

/// Find a variable's declared type from scope.
fn find_var_type(name: &str, scope: &Scope) -> Option<String> {
    for (n, ty) in &scope.params {
        if n == name {
            return Some(strip_direction(ty));
        }
    }
    for (n, ty) in &scope.locals {
        if n == name {
            return Some(strip_direction(ty));
        }
    }
    None
}

/// Strip direction prefix ("in ", "out ", "inout ") from type string.
fn strip_direction(ty: &str) -> String {
    let t = ty.trim();
    if t.starts_with("inout ") {
        t[6..].to_string()
    } else if t.starts_with("out ") {
        t[4..].to_string()
    } else if t.starts_with("in ") {
        t[3..].to_string()
    } else {
        t.to_string()
    }
}

/// Given a type name and a field name, resolve the field's type.
fn resolve_field_type(
    type_name: &str,
    field_name: &str,
    tree: &Tree,
    source: &str,
    workspace: &WorkspaceIndex,
    _uri: &Url,
) -> Option<String> {
    let target = type_name.trim();

    // Search current file first
    if let Some(node) = find_type_def_node(tree, source, target) {
        if let Some(result) = find_field_type_in_node(node, field_name, source) {
            return Some(result);
        }
    }

    // Search all workspace files
    for entry in workspace.files.iter() {
        let file_index = entry.value();
        if let Some(ref file_tree) = file_index.tree {
            if let Some(node) = find_type_def_node(file_tree, &file_index.source, target) {
                if let Some(result) = find_field_type_in_node(node, field_name, &file_index.source) {
                    return Some(result);
                }
            }
        }
    }

    // Built-in type field resolution
    resolve_builtin_field_type(type_name, field_name)
}

/// Find a type definition node (header, struct, extern, etc.) in a tree.
fn find_type_def_node<'a>(tree: &'a Tree, source: &'a str, type_name: &str) -> Option<Node<'a>> {
    let root = tree.root_node();
    let target = type_name.trim();
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            let kind = child.kind();
            if kind == "header_definition"
                || kind == "header_union_definition"
                || kind == "struct_definition"
                || kind == "extern_definition"
            {
                if let Some(name) = type_name_node(child, source) {
                    if name == target {
                        return Some(child);
                    }
                }
            }
        }
    }
    None
}

/// Find the type of a field within a definition node.
fn find_field_type_in_node(node: Node, field_name: &str, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "field" {
                if let Some(name) = field_name_node(child, source) {
                    if name == field_name {
                        return field_type_text(child, source);
                    }
                }
            }
        }
    }
    None
}

/// Extract field name from a field node.
fn field_name_node(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "identifier" {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

/// Extract type text from a field node.
fn field_type_text(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let k = child.kind();
            if k == "type_identifier"
                || k == "bit_type"
                || k == "varbit_type"
                || k == "tuple_type"
                || k == "bool"
                || k == "int"
                || k == "bit"
                || k == "varbit"
                || k == "error"
                || k == "packet_in"
                || k == "packet_out"
            {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

/// Resolve field types for built-in / standard types.
fn resolve_builtin_field_type(type_name: &str, field_name: &str) -> Option<String> {
    match type_name {
        "packet_in" => match field_name {
            "extract" | "lookAhead" | "advance" | "length" => Some("method".to_string()),
            _ => None,
        },
        "packet_out" => match field_name {
            "emit" => Some("method".to_string()),
            _ => None,
        },
        "standard_metadata_t" => Some("bit_type".to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Type completions: given a type name, return its fields and methods
// ---------------------------------------------------------------------------

fn type_completions(
    type_name: &str,
    tree: &Tree,
    source: &str,
    workspace: &WorkspaceIndex,
    _uri: &Url,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let target = type_name.trim();

    // Search current document's AST for the type definition
    if let Some(node) = find_type_def_node(tree, source, target) {
        let kind = node.kind();
        if kind == "header_definition"
            || kind == "header_union_definition"
            || kind == "struct_definition"
        {
            add_fields_as_completions(node, source, &mut items);
        } else if kind == "extern_definition" {
            add_methods_as_completions(node, source, &mut items);
        }
    }

    // Search workspace files
    for entry in workspace.files.iter() {
        let file_index = entry.value();
        if let Some(ref file_tree) = file_index.tree {
            if let Some(node) = find_type_def_node(file_tree, &file_index.source, target) {
                let kind = node.kind();
                if kind == "header_definition"
                    || kind == "header_union_definition"
                    || kind == "struct_definition"
                {
                    add_fields_as_completions(node, &file_index.source, &mut items);
                } else if kind == "extern_definition" {
                    add_methods_as_completions(node, &file_index.source, &mut items);
                }
                break; // Found the type, no need to search further
            }
        }
    }

    items
}

fn add_fields_as_completions(node: Node, source: &str, items: &mut Vec<CompletionItem>) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "field" {
                if let (Some(name), Some(ty)) = (field_name_node(child, source), field_type_text(child, source)) {
                    items.push(CompletionItem {
                        label: name,
                        kind: Some(CompletionItemKind::FIELD),
                        detail: Some(ty),
                        ..Default::default()
                    });
                }
            }
        }
    }
}

fn add_methods_as_completions(node: Node, source: &str, items: &mut Vec<CompletionItem>) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "method" {
                if let Some(item) = method_to_completion(child, source) {
                    items.push(item);
                }
            }
        }
    }
}

fn method_to_completion(node: Node, source: &str) -> Option<CompletionItem> {
    let name = child_text_by_kind(node, "method_identifier", source)?;
    let params = parameters_summary(node, source);
    let ret = return_type_text(node, source).unwrap_or_else(|| "void".to_string());
    Some(CompletionItem {
        label: name.clone(),
        kind: Some(CompletionItemKind::METHOD),
        detail: Some(format!("{} {}({})", ret, name, params)),
        ..Default::default()
    })
}

fn return_type_text(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let k = child.kind();
            if k == "type_identifier"
                || k == "bit_type"
                || k == "varbit_type"
                || k == "tuple_type"
                || k == "bool"
                || k == "int"
                || k == "bit"
                || k == "varbit"
                || k == "error"
                || k == "packet_in"
                || k == "packet_out"
            {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

fn parameters_summary(node: Node, source: &str) -> String {
    let mut params = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "parameter" {
                if let Some(p) = parameter_summary(child, source) {
                    params.push(p);
                }
            }
        }
    }
    params.join(", ")
}

fn parameter_summary(node: Node, source: &str) -> Option<String> {
    let mut dir = String::new();
    let mut ty = String::new();
    let mut name = String::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let k = child.kind();
            if k == "direction" {
                dir = format!("{} ", node_text(child, source));
            }
            if k == "identifier" || k == "type_identifier" {
                if ty.is_empty() && (k == "type_identifier" || is_type_like(child, source)) {
                    ty = node_text(child, source).to_string();
                } else {
                    name = node_text(child, source).to_string();
                }
            }
            if k == "bit_type"
                || k == "varbit_type"
                || k == "tuple_type"
                || k == "bool"
                || k == "int"
                || k == "bit"
                || k == "varbit"
                || k == "error"
                || k == "packet_in"
                || k == "packet_out"
            {
                ty = node_text(child, source).to_string();
            }
        }
    }
    if name.is_empty() {
        if !ty.is_empty() {
            return Some(format!("{}{}", dir, ty));
        }
        return None;
    }
    Some(format!("{}{} {}", dir, ty, name))
}

fn is_type_like(node: Node, source: &str) -> bool {
    let text = node_text(node, source);
    matches!(
        text,
        "bool" | "int" | "bit" | "varbit" | "error" | "packet_in" | "packet_out" | "string"
    )
}

fn type_name_node(node: Node, source: &str) -> Option<String> {
    child_text_by_kind(node, "type_identifier", source)
        .or_else(|| child_text_by_kind(node, "method_identifier", source))
        .or_else(|| child_text_by_kind(node, "identifier", source))
}

// ---------------------------------------------------------------------------
// Built-in methods for known types
// ---------------------------------------------------------------------------

fn add_builtin_methods(type_name: &str, items: &mut Vec<CompletionItem>) {
    match type_name.trim() {
        "packet_in" => {
            items.push(method_item("extract", "void extract(T header)"));
            items.push(method_item("lookAhead", "T lookAhead<T>()"));
            items.push(method_item("advance", "void advance(bit<32> sizeInBits)"));
            items.push(method_item("length", "bit<32> length()"));
        }
        "packet_out" => {
            items.push(method_item("emit", "void emit(T data)"));
        }
        "header" | "header_union" => {
            items.push(method_item("isValid", "bool isValid()"));
            items.push(method_item("setValid", "void setValid()"));
            items.push(method_item("setInvalid", "void setInvalid()"));
        }
        _ => {}
    }
}

fn method_item(label: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::METHOD),
        detail: Some(detail.to_string()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Scope completions
// ---------------------------------------------------------------------------

fn add_scope_completions(items: &mut Vec<CompletionItem>, scope: &Scope) {
    for (name, ty) in &scope.params {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(format!("parameter: {}", ty)),
            ..Default::default()
        });
    }
    for (name, ty) in &scope.locals {
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(format!("variable: {}", ty)),
            ..Default::default()
        });
    }
}

// ---------------------------------------------------------------------------
// Global completions
// ---------------------------------------------------------------------------

fn add_global_completions(
    items: &mut Vec<CompletionItem>,
    workspace: &WorkspaceIndex,
    _current_uri: &Url,
    prefix: Option<&str>,
) {
    // Collect all symbols across files
    let mut seen = std::collections::HashSet::new();

    for entry in workspace.files.iter() {
        let (_uri, file_index) = (entry.key(), entry.value());
        for sym in &file_index.symbols {
            if let Some(p) = prefix {
                if !sym.name.starts_with(p) {
                    continue;
                }
            }
            if seen.insert(sym.name.clone()) {
                let kind = symbol_kind_to_completion_kind(sym.kind);
                items.push(CompletionItem {
                    label: sym.name.clone(),
                    kind: Some(kind),
                    detail: Some(format!("{:?}", sym.kind)),
                    ..Default::default()
                });
            }
        }
    }
}

fn symbol_kind_to_completion_kind(kind: tower_lsp::lsp_types::SymbolKind) -> CompletionItemKind {
    match kind {
        tower_lsp::lsp_types::SymbolKind::FUNCTION => CompletionItemKind::FUNCTION,
        tower_lsp::lsp_types::SymbolKind::CLASS => CompletionItemKind::CLASS,
        tower_lsp::lsp_types::SymbolKind::STRUCT => CompletionItemKind::STRUCT,
        tower_lsp::lsp_types::SymbolKind::ENUM => CompletionItemKind::ENUM,
        tower_lsp::lsp_types::SymbolKind::INTERFACE => CompletionItemKind::INTERFACE,
        tower_lsp::lsp_types::SymbolKind::CONSTANT => CompletionItemKind::CONSTANT,
        tower_lsp::lsp_types::SymbolKind::OBJECT => CompletionItemKind::REFERENCE,
        tower_lsp::lsp_types::SymbolKind::TYPE_PARAMETER => CompletionItemKind::TYPE_PARAMETER,
        _ => CompletionItemKind::TEXT,
    }
}

// ---------------------------------------------------------------------------
// Keyword completions
// ---------------------------------------------------------------------------

fn add_keyword_completions(items: &mut Vec<CompletionItem>) {
    let keywords = [
        ("action", "Define an action"),
        ("apply", "Control apply block"),
        ("bool", "Boolean type"),
        ("bit", "Bit vector type"),
        ("const", "Constant declaration"),
        ("control", "Control block"),
        ("default", "Default case"),
        ("else", "Else branch"),
        ("enum", "Enumeration"),
        ("error", "Error type"),
        ("extern", "Extern declaration"),
        ("false", "Boolean false"),
        ("for", "For loop"),
        ("header", "Header definition"),
        ("header_union", "Header union definition"),
        ("if", "Conditional"),
        ("in", "Input direction"),
        ("inout", "Inout direction"),
        ("int", "Signed integer"),
        ("out", "Output direction"),
        ("package", "Package declaration"),
        ("parser", "Parser block"),
        ("return", "Return statement"),
        ("select", "Select expression"),
        ("state", "Parser state"),
        ("struct", "Struct definition"),
        ("switch", "Switch statement"),
        ("table", "Table definition"),
        ("this", "This reference"),
        ("transition", "State transition"),
        ("true", "Boolean true"),
        ("tuple", "Tuple type"),
        ("typedef", "Type alias"),
        ("varbit", "Variable bit vector"),
        ("void", "Void type"),
    ];

    for (kw, detail) in &keywords {
        items.push(CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(detail.to_string()),
            ..Default::default()
        });
    }
}

// ---------------------------------------------------------------------------
// Standard library completions
// ---------------------------------------------------------------------------

fn add_stdlib_completions(
    items: &mut Vec<CompletionItem>,
    scope: &Scope,
    tree: &Tree,
    source: &str,
    workspace: &WorkspaceIndex,
    uri: &Url,
) {
    // If scope has packet_in / packet_out variables, add their methods
    for (name, ty) in &scope.params {
        let clean_ty = strip_direction(ty);
        if clean_ty == "packet_in" || clean_ty == "packet_out" {
            let mut type_items = Vec::new();
            add_builtin_methods(&clean_ty, &mut type_items);
            for mut item in type_items {
                item.detail = Some(format!("{}.{}", name, item.label));
                items.push(item);
            }
        }
        // If type is a header type, add header methods
        if is_header_type(&clean_ty, tree, source, workspace, uri) {
            items.push(method_item("isValid", "bool isValid()"));
            items.push(method_item("setValid", "void setValid()"));
            items.push(method_item("setInvalid", "void setInvalid()"));
        }
    }

    // standard_metadata_t fields
    items.push(CompletionItem {
        label: "standard_metadata_t".to_string(),
        kind: Some(CompletionItemKind::STRUCT),
        detail: Some("Standard metadata struct".to_string()),
        ..Default::default()
    });
}

/// Check if a type name refers to a header (or header_union) definition.
fn is_header_type(
    type_name: &str,
    tree: &Tree,
    source: &str,
    workspace: &WorkspaceIndex,
    _uri: &Url,
) -> bool {
    // Check current file
    if let Some(node) = find_type_def_node(tree, source, type_name) {
        let kind = node.kind();
        if kind == "header_definition" || kind == "header_union_definition" {
            return true;
        }
    }

    // Search workspace files
    for entry in workspace.files.iter() {
        let file_index = entry.value();
        if let Some(ref file_tree) = file_index.tree {
            if let Some(node) = find_type_def_node(file_tree, &file_index.source, type_name) {
                let kind = node.kind();
                if kind == "header_definition" || kind == "header_union_definition" {
                    return true;
                }
            }
        }
    }

    false
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use crate::workspace::WorkspaceIndex;
    use tower_lsp::lsp_types::{Position, Url};

    fn parse_p4(source: &str) -> Tree {
        let mut p = parser::new_parser().unwrap();
        p.parse(source, None).expect("parse should succeed")
    }

    fn setup(source: &str) -> (Tree, WorkspaceIndex, Url) {
        let tree = parse_p4(source);
        let workspace = WorkspaceIndex::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        workspace.index_document(&uri, &tree, source);
        (tree, workspace, uri)
    }

    // 辅助函数：从补全列表中提取所有 label
    fn labels(items: &[CompletionItem]) -> Vec<String> {
        items.iter().map(|i| i.label.clone()).collect()
    }

    /// 测试 1：在 action 参数位置触发补全，验证返回局部作用域符号（参数名）。
    #[test]
    fn test_action_param_completion() {
        let source = "control C() {\n    action a(in bit<32> p, inout bit<16> q) {\n        p;\n    }\n    apply {}\n}";
        let (tree, workspace, uri) = setup(source);
        // 光标放在 action body 中 "p;" 的 'p' 位置（line 2, character 8）
        let pos = Position { line: 2, character: 8 };
        let items = completions(&uri, pos, &tree, source, &workspace, None);
        let labs = labels(&items);

        assert!(labs.contains(&"p".to_string()), "should contain param 'p'");
        assert!(labs.contains(&"q".to_string()), "should contain param 'q'");
    }

    /// 测试 2：在 control 的局部变量位置触发补全，验证返回 locals。
    #[test]
    fn test_control_local_completion() {
        let source = "control C() {\n    bit<32> x = 0;\n    apply {\n        x;\n    }\n}";
        let (tree, workspace, uri) = setup(source);
        // 光标放在 apply 块内 "x;" 的 'x' 位置（line 3, character 8）
        let pos = Position { line: 3, character: 8 };
        let items = completions(&uri, pos, &tree, source, &workspace, None);
        let labs = labels(&items);

        assert!(labs.contains(&"x".to_string()), "should contain local var 'x'");
    }

    /// 测试 3：对 `.` 触发补全，左侧是 header 类型时，验证返回字段和 isValid/setInvalid 等方法。
    #[test]
    fn test_header_dot_completion() {
        let source = "header H { bit<32> f; }\ncontrol C(inout H h) {\n    apply {\n        h.\n    }\n}";
        let (tree, workspace, uri) = setup(source);
        // 光标放在 "h." 之后（line 3, character 10）
        let pos = Position { line: 3, character: 10 };
        let items = completions(&uri, pos, &tree, source, &workspace, Some("."));
        let labs = labels(&items);

        assert!(labs.contains(&"f".to_string()), "should contain field 'f'");
        assert!(labs.contains(&"isValid".to_string()), "should contain header method 'isValid'");
        assert!(labs.contains(&"setValid".to_string()), "should contain header method 'setValid'");
        assert!(labs.contains(&"setInvalid".to_string()), "should contain header method 'setInvalid'");
    }

    /// 测试 4：对 `.` 触发补全，左侧是 packet_in 类型时，验证返回 extract/lookAhead 等方法。
    #[test]
    fn test_packet_in_dot_completion() {
        let source = "parser P(packet_in pkt) {\n    state start {\n        pkt.\n    }\n}";
        let (tree, workspace, uri) = setup(source);
        // 光标放在 "pkt." 之后（line 2, character 12）
        let pos = Position { line: 2, character: 12 };
        let items = completions(&uri, pos, &tree, source, &workspace, Some("."));
        let labs = labels(&items);

        assert!(labs.contains(&"extract".to_string()), "should contain 'extract'");
        assert!(labs.contains(&"lookAhead".to_string()), "should contain 'lookAhead'");
        assert!(labs.contains(&"advance".to_string()), "should contain 'advance'");
        assert!(labs.contains(&"length".to_string()), "should contain 'length'");
    }

    /// 测试 5：对 `.` 触发补全，左侧是 standard_metadata_t 类型时，验证返回 ingress_port/egress_port 等字段。
    #[test]
    fn test_standard_metadata_dot_completion() {
        let source = "struct standard_metadata_t {\n    bit<9> ingress_port;\n    bit<9> egress_port;\n    bit<32> packet_length;\n}\ncontrol C(inout standard_metadata_t meta) {\n    apply {\n        meta.\n    }\n}";
        let (tree, workspace, uri) = setup(source);
        // 光标放在 "meta." 之后（line 7, character 13）
        let pos = Position { line: 7, character: 13 };
        let items = completions(&uri, pos, &tree, source, &workspace, Some("."));
        let labs = labels(&items);

        assert!(labs.contains(&"ingress_port".to_string()), "should contain 'ingress_port'");
        assert!(labs.contains(&"egress_port".to_string()), "should contain 'egress_port'");
        assert!(labs.contains(&"packet_length".to_string()), "should contain 'packet_length'");
    }

    /// 测试 6：全局位置（非 `.`）补全，验证返回 P4 关键字（if, else, return, switch）。
    #[test]
    fn test_keyword_completion() {
        let source = "control C() {\n    apply {\n        \n    }\n}";
        let (tree, workspace, uri) = setup(source);
        // 光标放在 apply 块空白处（line 2, character 8）
        let pos = Position { line: 2, character: 8 };
        let items = completions(&uri, pos, &tree, source, &workspace, None);
        let labs = labels(&items);

        assert!(labs.contains(&"if".to_string()), "should contain keyword 'if'");
        assert!(labs.contains(&"else".to_string()), "should contain keyword 'else'");
        assert!(labs.contains(&"return".to_string()), "should contain keyword 'return'");
        assert!(labs.contains(&"switch".to_string()), "should contain keyword 'switch'");
    }

    // ========================================================================
    // 嵌套多层结构 completion 测试
    // ========================================================================

    /// 测试 7：嵌套 struct 字段补全 — struct A { struct B b; } 前置声明后 obj.b. 应返回 B 的字段。
    #[test]
    fn test_nested_struct_field_completion() {
        let source = r#"struct B {
    bit<32> x;
    bit<16> y;
}
struct A {
    B b;
}
control C(inout A obj) {
    apply {
        obj.b.
    }
}"#;
        let (tree, workspace, uri) = setup(source);
        // 光标放在 apply 块内 "obj.b." 之后（line 9, character 14）
        let pos = Position { line: 9, character: 14 };
        
        // Debug: check scope
        let _scope = workspace.scope_at(&uri, pos, &tree, source);
        
        let items = completions(&uri, pos, &tree, source, &workspace, Some("."));
        let labs = labels(&items);

        assert!(labs.contains(&"x".to_string()), "should contain nested field x");
        assert!(labs.contains(&"y".to_string()), "should contain nested field y");
    }

    /// 测试 8：多层 control 局部变量补全 — if 块内变量在 if 块内可见、在 if 块外不可见。
    #[test]
    fn test_nested_block_scope_completion() {
        let source = r#"control C() {
    apply {
        bit<32> outer = 0;
        if (true) {
            bit<16> inner = 1;
            inner;
        }
        outer;
    }
}"#;
        let (tree, workspace, uri) = setup(source);

        // 光标在 if 块内 inner 使用处（line 5, character 12）
        let pos_inside = Position { line: 5, character: 12 };
        let items_inside = completions(&uri, pos_inside, &tree, source, &workspace, None);
        let labs_inside = labels(&items_inside);
        assert!(labs_inside.contains(&"outer".to_string()), "inside-if should see outer");
        assert!(labs_inside.contains(&"inner".to_string()), "inside-if should see inner");

        // 光标在 if 块外 outer 使用处（line 7, character 8）
        let pos_outside = Position { line: 7, character: 8 };
        let items_outside = completions(&uri, pos_outside, &tree, source, &workspace, None);
        let labs_outside = labels(&items_outside);
        assert!(labs_outside.contains(&"outer".to_string()), "outside-if should see outer");
        assert!(
            !labs_outside.contains(&"inner".to_string()),
            "outside-if should NOT see inner"
        );
    }

    /// 测试 9：table apply 后局部变量补全 — table.apply() 之后声明的变量应在后续代码中补全。
    #[test]
    fn test_table_apply_then_local_completion() {
        let source = r#"control C() {
    table ipv4_table {
        actions = { a; }
    }
    action a() {}
    apply {
        ipv4_table.apply();
        bit<32> after_var = 0;
        after_var;
    }
}"#;
        let (tree, workspace, uri) = setup(source);
        // 光标放在 after_var 使用处（line 8, character 8）
        let pos = Position { line: 8, character: 8 };
        let items = completions(&uri, pos, &tree, source, &workspace, None);
        let labs = labels(&items);

        assert!(
            labs.contains(&"after_var".to_string()),
            "should contain local var after_var after table.apply()"
        );
    }

    /// 测试 10：parser state 内嵌套局部变量补全（P4 无 for，用 parser state 嵌套替代）。
    #[test]
    fn test_parser_state_nested_local_completion() {
        let source = r#"parser P(packet_in pkt) {
    state start {
        bit<32> state_local = 0;
        if (true) {
            bit<16> if_local = 1;
            if_local;
        }
        state_local;
        transition accept;
    }
}"#;
        let (tree, workspace, uri) = setup(source);

        // 光标在 if 块内 if_local 使用处（line 5, character 12）
        let pos_inside = Position { line: 5, character: 12 };
        let items_inside = completions(&uri, pos_inside, &tree, source, &workspace, None);
        let labs_inside = labels(&items_inside);
        assert!(
            labs_inside.contains(&"state_local".to_string()),
            "inside-if should see parser state local state_local"
        );
        assert!(
            labs_inside.contains(&"if_local".to_string()),
            "inside-if should see if_local"
        );

        // 光标在 if 块外 state_local 使用处（line 7, character 8）
        let pos_outside = Position { line: 7, character: 8 };
        let items_outside = completions(&uri, pos_outside, &tree, source, &workspace, None);
        let labs_outside = labels(&items_outside);
        assert!(
            labs_outside.contains(&"state_local".to_string()),
            "outside-if should see state_local"
        );
        assert!(
            !labs_outside.contains(&"if_local".to_string()),
            "outside-if should NOT see if_local"
        );
    }

    /// 测试 11：嵌套 action 调用补全 — action 内调用另一个 action，参数位置应能补全局部作用域符号。
    #[test]
    fn test_nested_action_call_param_completion() {
        let source = r#"control C() {
    action inner_action(bit<32> p) {
        bit<16> local_x = 1;
        // cursor here should see p and local_x
    }
    action outer_action() {
        bit<32> arg_val = 42;
        inner_action(arg_val);
    }
    apply {}
}"#;
        let (tree, workspace, uri) = setup(source);
        // 光标放在 inner_action body 中（line 3, character 12）
        let pos = Position { line: 3, character: 12 };
        let items = completions(&uri, pos, &tree, source, &workspace, None);
        let labs = labels(&items);

        assert!(
            labs.contains(&"p".to_string()),
            "inner_action scope should contain param p"
        );
        assert!(
            labs.contains(&"local_x".to_string()),
            "inner_action scope should contain local local_x"
        );

        // 光标放在 outer_action 调用 inner_action 的参数位置（line 7, character 20）
        let pos_call = Position { line: 7, character: 20 };
        let items_call = completions(&uri, pos_call, &tree, source, &workspace, None);
        let labs_call = labels(&items_call);
        assert!(
            labs_call.contains(&"arg_val".to_string()),
            "outer_action call site should see local arg_val"
        );
    }


    /// 测试 12：跨文件字段补全 — 类型定义在文件 A 中，在文件 B 中使用，dot-triggered completion 应能解析字段。
    #[test]
    fn test_cross_file_field_completion() {
        let source_a = r#"header ethernet_t {
    bit<48> dst_addr;
    bit<48> src_addr;
    bit<16> ether_type;
}
"#;
        let source_b = r#"control C(inout ethernet_t eth) {
    apply {
        eth.
    }
}
"#;

        let tree_a = parse_p4(source_a);
        let tree_b = parse_p4(source_b);
        let workspace = WorkspaceIndex::new();
        let uri_a = Url::parse("file:///a.p4").unwrap();
        let uri_b = Url::parse("file:///b.p4").unwrap();
        workspace.index_document(&uri_a, &tree_a, source_a);
        workspace.index_document(&uri_b, &tree_b, source_b);

        let pos = Position { line: 2, character: 12 };
        let items = completions(&uri_b, pos, &tree_b, source_b, &workspace, Some("."));
        let labs = labels(&items);

        // Debug: print all labels
        eprintln!("Cross-file completion labels: {:?}", labs);

        assert!(
            labs.contains(&"dst_addr".to_string()),
            "should contain cross-file field 'dst_addr'"
        );
        assert!(
            labs.contains(&"src_addr".to_string()),
            "should contain cross-file field 'src_addr'"
        );
        assert!(
            labs.contains(&"ether_type".to_string()),
            "should contain cross-file field 'ether_type'"
        );
        assert!(
            labs.contains(&"isValid".to_string()),
            "should contain header method 'isValid' (cross-file header type detected)"
        );
    }

    /// 快速 AST 探测：打印光标位置到根节点的路径
    #[test]
    fn probe_ast_simple() {
        let src = "control C() {\n    apply {\n        bit<32> outer = 0;\n        if (true) {\n            bit<16> inner = 1;\n        }\n    }\n}";
        let tree = parse_p4(src);
        let point = tree_sitter::Point { row: 4, column: 12 };
        if let Some(node) = tree.root_node().descendant_for_point_range(point, point) {
            let mut current = Some(node);
            while let Some(n) = current {
                current = n.parent();
            }
        }
    }
}
