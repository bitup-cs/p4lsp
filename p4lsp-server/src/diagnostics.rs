use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};
use tree_sitter::Tree;

/// 从 tree-sitter Tree 中提取语法错误作为 LSP Diagnostic。
pub fn tree_diagnostics(tree: &Tree, source: &str, _uri: &Url) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let root = tree.root_node();
    walk_errors(root, source, &mut diagnostics);
    diagnostics
}

fn walk_errors(node: tree_sitter::Node, source: &str, out: &mut Vec<Diagnostic>) {
    if node.is_error() || node.is_missing() {
        let range = Range {
            start: Position {
                line: node.start_position().row as u32,
                character: node.start_position().column as u32,
            },
            end: Position {
                line: node.end_position().row as u32,
                character: node.end_position().column as u32,
            },
        };
        // Skip preprocessor-related errors (#include, #define, #ifndef, etc.)
        let text = &source[node.start_byte()..node.end_byte()];
        let is_preprocessor = text.trim_start().starts_with('#');
        if is_preprocessor {
            return;
        }
        let message = if node.is_missing() {
            format!("missing {}", node.kind())
        } else {
            format!("syntax error near '{}'", node.kind())
        };
        out.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("p4lsp".to_string()),
            message,
            related_information: None,
            tags: None,
            data: None,
        });
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_errors(child, source, out);
        }
    }
}

// ---------------------------------------------------------------------------
// 语义诊断：重复定义 + 未定义引用
// ---------------------------------------------------------------------------

use crate::workspace::WorkspaceIndex;
use tower_lsp::lsp_types::Url as LspUrl;

/// 收集语义诊断（重复定义 + 未定义引用）。
pub fn semantic_diagnostics(
    tree: &Tree,
    source: &str,
    uri: &LspUrl,
    index: &WorkspaceIndex,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let root = tree.root_node();

    // 1. 重复定义检测
    check_duplicate_definitions(root, source, &mut diagnostics);

    // 2. 未定义引用检测
    check_undefined_references(root, source, uri, tree, index, &mut diagnostics);

    // 3. 类型检查
    let type_diags = crate::typecheck::type_check(tree, source);
    diagnostics.extend(type_diags);

    diagnostics
}

// ---------------------------------------------------------------------------
// 重复定义检测
// ---------------------------------------------------------------------------

fn check_duplicate_definitions(
    node: tree_sitter::Node,
    source: &str,
    out: &mut Vec<Diagnostic>,
) {
    // 顶层：检查全局定义重复
    if node.kind() == "source_file" {
        let mut names: std::collections::HashMap<String, Vec<(u32, u32)>> = std::collections::HashMap::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if let Some((name, line, col)) = extract_definition_name(child, source) {
                    names.entry(name).or_default().push((line, col));
                }
            }
        }
        for (name, positions) in &names {
            if positions.len() > 1 {
                for (line, col) in positions {
                    out.push(Diagnostic {
                        range: Range {
                            start: Position { line: *line, character: *col },
                            end: Position { line: *line, character: *col + name.len() as u32 },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: None,
                        code_description: None,
                        source: Some("p4lsp".to_string()),
                        message: format!("duplicate definition of '{}'", name),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
            }
        }
    }

    // 递归检查子作用域中的重复定义
    // TODO: parser/control/action 内部的作用域重复定义检测

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            check_duplicate_definitions(child, source, out);
        }
    }
}

fn extract_definition_name(node: tree_sitter::Node, source: &str) -> Option<(String, u32, u32)> {
    let kind = node.kind();
    let name = match kind {
        "header_definition" => child_text_by_kind(node, "type_identifier", source)?,
        "struct_definition" => child_text_by_kind(node, "type_identifier", source)?,
        "enum_definition" => child_text_by_kind(node, "type_identifier", source)?,
        "parser_definition" => child_text_by_kind(node, "type_identifier", source)?,
        "control_definition" => child_text_by_kind(node, "type_identifier", source)?,
        "action" => child_text_by_kind(node, "method_identifier", source)?,
        "extern_definition" => child_text_by_kind(node, "type_identifier", source)?,
        "function_declaration" => child_text_by_kind(node, "method_identifier", source)?,
        "typedef_definition" => child_text_by_kind(node, "type_identifier", source)?,
        "type_definition" => child_text_by_kind(node, "type_identifier", source)?,
        "const_definition" => child_text_by_kind(node, "identifier", source)?,
        "table" => child_text_by_kind(node, "type_identifier", source)?,
        _ => return None,
    };
    let start = node.start_position();
    // 在节点内查找 identifier 的精确位置
    let mut id_node = None;
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            if c.kind() == "type_identifier" || c.kind() == "method_identifier" || c.kind() == "identifier" {
                id_node = Some(c);
                break;
            }
        }
    }
    if let Some(id) = id_node {
        let pos = id.start_position();
        Some((name, pos.row as u32, pos.column as u32))
    } else {
        Some((name, start.row as u32, start.column as u32))
    }
}

fn child_text_by_kind(node: tree_sitter::Node, kind: &str, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            if c.kind() == kind {
                return Some(source[c.start_byte()..c.end_byte()].to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 未定义引用检测
// ---------------------------------------------------------------------------

fn check_undefined_references(
    node: tree_sitter::Node,
    source: &str,
    uri: &LspUrl,
    tree: &Tree,
    index: &WorkspaceIndex,
    out: &mut Vec<Diagnostic>,
) {
    let kind = node.kind();

    // 只处理 identifier 节点（排除定义节点中的 identifier）
    if kind == "identifier" || kind == "type_identifier" {
        // 跳过定义节点内部的 identifier
        if let Some(parent) = node.parent() {
            let is_definition = is_definition_context(node, parent);
            if !is_definition {
                let name = source[node.start_byte()..node.end_byte()].to_string();

                // 检查是否为 P4 关键字或内置类型
                if is_p4_builtin(&name) {
                    return;
                }

                // 检查局部作用域
                let pos = Position {
                    line: node.start_position().row as u32,
                    character: node.start_position().column as u32,
                };
                let scope = index.scope_at(uri, pos, tree, source);
                let in_scope = scope.params.iter().any(|(n, _)| n == &name)
                    || scope.locals.iter().any(|(n, _)| n == &name);

                if !in_scope {
                    // 检查全局索引
                    let global = index.resolve_symbol(&name, uri);
                    if global.is_empty() {
                        out.push(Diagnostic {
                            range: Range {
                                start: Position {
                                    line: node.start_position().row as u32,
                                    character: node.start_position().column as u32,
                                },
                                end: Position {
                                    line: node.end_position().row as u32,
                                    character: node.end_position().column as u32,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: None,
                            code_description: None,
                            source: Some("p4lsp".to_string()),
                            message: format!("undefined reference to '{}'", name),
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                }
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            check_undefined_references(child, source, uri, tree, index, out);
        }
    }
}

/// 判断 identifier 节点是否处于定义上下文中（是定义名而非引用）。
fn is_definition_context(_node: tree_sitter::Node, parent: tree_sitter::Node) -> bool {
    let parent_kind = parent.kind();

    // 直接父节点就是定义节点
    if matches!(
        parent_kind,
        "header_definition"
            | "struct_definition"
            | "enum_definition"
            | "parser_definition"
            | "control_definition"
            | "action"
            | "extern_definition"
            | "function_declaration"
            | "typedef_definition"
            | "type_definition"
            | "const_definition"
            | "table"
            | "parameter"
            | "variable_declaration"
            | "var_decl"
            | "field"
    ) {
        return true;
    }

    // method_identifier 包裹 identifier：检查祖父节点是否为定义
    if parent_kind == "method_identifier" {
        if let Some(grandparent) = parent.parent() {
            if matches!(
                grandparent.kind(),
                "control_definition"
                    | "parser_definition"
                    | "action"
                    | "function_declaration"
                    | "extern_definition"
                    | "method"
            ) {
                return true;
            }
        }
    }

    // type_identifier 包裹 identifier：检查祖父节点是否为定义
    if parent_kind == "type_identifier" {
        if let Some(grandparent) = parent.parent() {
            if matches!(
                grandparent.kind(),
                "header_definition"
                    | "struct_definition"
                    | "enum_definition"
                    | "typedef_definition"
                    | "type_definition"
                    | "table"
                    | "extern_definition"
            ) {
                return true;
            }
        }
    }

    // lval 在 var_decl 中表示变量声明的左值（变量名）
    if parent_kind == "lval" {
        if let Some(grandparent) = parent.parent() {
            if grandparent.kind() == "var_decl" {
                return true;
            }
        }
    }

    false
}

fn is_p4_builtin(name: &str) -> bool {
    matches!(
        name,
        "bit"
            | "int"
            | "bool"
            | "varbit"
            | "string"
            | "void"
            | "error"
            | "match_kind"
            | "packet_in"
            | "packet_out"
            | "in"
            | "out"
            | "inout"
            | "optional"
            | "const"
            | "return"
            | "if"
            | "else"
            | "switch"
            | "for"
            | "while"
            | "do"
            | "break"
            | "continue"
            | "true"
            | "false"
            | "this"
            | "default"
    )
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn parse_source(source: &str) -> Tree {
        let mut parser = parser::new_parser().unwrap();
        parser.parse(source, None).expect("parse should succeed")
    }

    #[test]
    fn test_duplicate_definition() {
        let source = r#"
header h { bit<8> a; }
header h { bit<16> b; }
"#;
        let tree = parse_source(source);
        let mut diags = Vec::new();
        check_duplicate_definitions(tree.root_node(), source, &mut diags);
        assert_eq!(diags.len(), 2, "should report 2 duplicate definition errors");
        assert!(diags[0].message.contains("duplicate definition of 'h'"));
    }

    #[test]
    fn test_undefined_reference() {
        let source = r#"
control C() {
    apply {
        bit<8> x = undefined_var;
    }
}
"#;
        let tree = parse_source(source);
        let mut diags = Vec::new();
        let index = WorkspaceIndex::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        check_undefined_references(tree.root_node(), source, &uri, &tree, &index, &mut diags);
        assert_eq!(diags.len(), 1, "should report 1 undefined reference");
        assert!(diags[0].message.contains("undefined reference to 'undefined_var'"));
    }

    #[test]
    fn test_no_false_positive_for_defined() {
        let source = r#"
control C() {
    apply {
        bit<8> x = 1;
        bit<8> y = x;
    }
}
"#;
        let tree = parse_source(source);
        let mut diags = Vec::new();
        let index = WorkspaceIndex::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        check_undefined_references(tree.root_node(), source, &uri, &tree, &index, &mut diags);
        assert!(diags.is_empty(), "should not report undefined for local variable 'x'");
    }
}
