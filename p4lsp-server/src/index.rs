#![allow(deprecated)]

use tower_lsp::lsp_types::{DocumentSymbol, SymbolKind};
use tree_sitter::Node;

/// 从 tree-sitter AST 提取 DocumentSymbol 列表。
pub fn document_symbols(root: Node, source: &str) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            // top 节点包裹了实际定义或 preproc
            if child.kind() == "top" {
                if let Some(def) = child.child(0) {
                    if let Some(sym) = node_symbol(def, source) {
                        symbols.push(sym);
                    }
                }
            } else if let Some(sym) = node_symbol(child, source) {
                symbols.push(sym);
            }
        }
    }
    symbols
}

fn node_symbol(node: Node, source: &str) -> Option<DocumentSymbol> {
    let kind = match node.kind() {
        "header_definition" | "header_union_definition" => SymbolKind::STRUCT,
        "struct_definition" => SymbolKind::STRUCT,
        "enum_definition" => SymbolKind::ENUM,
        "error_definition" => SymbolKind::CONSTANT,
        "match_kind_definition" => SymbolKind::CONSTANT,
        "parser_definition" => SymbolKind::CLASS,
        "control_definition" => SymbolKind::CLASS,
        "extern_definition" => SymbolKind::INTERFACE,
        "function_declaration" => SymbolKind::FUNCTION,
        "action" | "annotated_action" => SymbolKind::FUNCTION,
        "table" | "annotated_table" => SymbolKind::OBJECT,
        "const_definition" => SymbolKind::CONSTANT,
        "typedef_definition" | "type_definition" => SymbolKind::TYPE_PARAMETER,
        "value_set_declaration" => SymbolKind::CONSTANT,
        _ => return None,
    };

    let name = annotated_inner_name(node, source)?;
    let range = ts_range_to_lsp(node);
    let selection_range = ts_range_to_lsp(node);

    // 提取子符号（例如 header 的字段、control 的 apply/action/table）
    let children = extract_children(node, source);

    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() { None } else { Some(children) },
    })
}

fn extract_children(node: Node, source: &str) -> Vec<DocumentSymbol> {
    let mut children = Vec::new();
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            let kind = child.kind();
            // header/struct/union 的字段
            if kind == "field" {
                if let Some(name) = field_name(child, source) {
                    children.push(DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::FIELD,
                        tags: None,
                        deprecated: None,
                        
                        range: ts_range_to_lsp(child),
                        selection_range: ts_range_to_lsp(child),
                        children: None,
                    });
                }
            }
            // control 内部的 action/table/apply (递归查找嵌套节点)
            if kind == "action" || kind == "table" || kind == "annotated_action" || kind == "annotated_table" {
                if let Some(sym) = node_symbol(child, source) {
                    children.push(sym);
                }
            }
            // 递归进入 control_body / control_body_element 等容器节点
            if kind == "control_body" || kind == "control_body_element" || kind == "parser_body" || kind == "parser_body_element" {
                children.extend(extract_children(child, source));
            }
            // parser 的 state
            if kind == "state" {
                if let Some(name) = state_name(child, source) {
                    children.push(DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::METHOD,
                        tags: None,
                        deprecated: None,
                        
                        range: ts_range_to_lsp(child),
                        selection_range: ts_range_to_lsp(child),
                        children: None,
                    });
                }
            }
            // parser 的 value_set
            if kind == "value_set_declaration" {
                if let Some(sym) = node_symbol(child, source) {
                    children.push(sym);
                }
            }
            // extern 的方法
            if kind == "method" {
                if let Some(name) = method_name(child, source) {
                    children.push(DocumentSymbol {
                        name,
                        detail: None,
                        kind: SymbolKind::METHOD,
                        tags: None,
                        deprecated: None,
                        
                        range: ts_range_to_lsp(child),
                        selection_range: ts_range_to_lsp(child),
                        children: None,
                    });
                }
            }
        }
    }
    children
}

fn node_name(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "type_identifier" || child.kind() == "method_identifier" || child.kind() == "identifier" {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

/// 提取 annotated_action / annotated_table 内部 action/table 节点的名称。
/// 如果 node 不是 annotated 类型，则直接返回 node 本身的名称。
pub fn annotated_inner_name(node: Node, source: &str) -> Option<String> {
    if node.kind() == "annotated_action" || node.kind() == "annotated_table" {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "action" || child.kind() == "table" {
                    return node_name(child, source);
                }
            }
        }
        None
    } else {
        node_name(node, source)
    }
}

fn field_name(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "identifier" {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

fn state_name(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "method_identifier" {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

fn method_name(node: Node, source: &str) -> Option<String> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "method_identifier" {
                return Some(node_text(child, source).to_string());
            }
        }
    }
    None
}

fn node_text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn ts_range_to_lsp(node: Node) -> tower_lsp::lsp_types::Range {
    tower_lsp::lsp_types::Range {
        start: tower_lsp::lsp_types::Position {
            line: node.start_position().row as u32,
            character: node.start_position().column as u32,
        },
        end: tower_lsp::lsp_types::Position {
            line: node.end_position().row as u32,
            character: node.end_position().column as u32,
        },
    }
}
