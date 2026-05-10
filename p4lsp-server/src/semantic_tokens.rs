use tower_lsp::lsp_types::{
    Position, Range, SemanticToken, SemanticTokens, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensServerCapabilities, WorkDoneProgressOptions,
};
use tree_sitter::{Node, Point};

/// Token types in order matching the legend indices.
const TOKEN_TYPES: &[&str] = &[
    "namespace", // 0
    "type",      // 1
    "function",  // 2
    "variable",  // 3
    "number",    // 4
    "string",    // 5
    "comment",   // 6
    "keyword",   // 7
];

const TOKEN_MODIFIERS: &[&str] = &[];

/// P4 keywords recognized by tree-sitter as individual node kinds.
const P4_KEYWORDS: &[&str] = &[
    // Type keywords
    "bit", "int", "bool", "varbit", "void", "string", "error", "match_kind",
    "packet_in", "packet_out",
    // Declaration keywords
    "header", "struct", "enum", "parser", "control", "action", "table", "extern",
    "function", "const", "typedef", "type", "package", "value_set",
    // Direction keywords
    "in", "out", "inout",
    // Statement keywords
    "if", "else", "for", "switch", "return", "apply", "transition", "state",
    // Other keywords
    "default", "true", "false", "exact", "lpm", "ternary", "optional", "range",
    "key", "actions", "entries", "size", "priority",
    // Annotation keyword
    "@", "_",
];

pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.iter().map(|s| s.to_string().into()).collect(),
        token_modifiers: TOKEN_MODIFIERS.iter().map(|s| s.to_string().into()).collect(),
    }
}

pub fn server_capabilities() -> SemanticTokensServerCapabilities {
    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        legend: legend(),
        range: Some(true),
        full: Some(tower_lsp::lsp_types::SemanticTokensFullOptions::Bool(true)),
    })
}

/// Build semantic tokens for the entire document.
pub fn semantic_tokens_full(tree: &tree_sitter::Tree, source: &str) -> SemanticTokens {
    let root = tree.root_node();
    let tokens = collect_tokens(root, source, None);
    encode_tokens(&tokens)
}

/// Build semantic tokens for a range.
pub fn semantic_tokens_range(
    tree: &tree_sitter::Tree,
    source: &str,
    range: Range,
) -> SemanticTokens {
    let root = tree.root_node();
    let tokens = collect_tokens(root, source, Some(range));
    encode_tokens(&tokens)
}

/// Internal representation of a semantic token before delta encoding.
#[derive(Debug, Clone, PartialEq)]
struct Token {
    line: u32,
    char: u32,
    length: u32,
    token_type: u32,
}

fn collect_tokens(node: Node, source: &str, range_filter: Option<Range>) -> Vec<Token> {
    let mut tokens = Vec::new();
    traverse(node, source, range_filter, &mut tokens);
    // Sort by (line, char)
    tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.char.cmp(&b.char)));
    tokens
}

fn traverse(node: Node, source: &str, range_filter: Option<Range>, out: &mut Vec<Token>) {
    // Check if this node overlaps with the range filter
    if let Some(range) = range_filter {
        let node_start = ts_point_to_lsp(node.start_position());
        let node_end = ts_point_to_lsp(node.end_position());
        if node_end.line < range.start.line
            || (node_end.line == range.start.line && node_end.character < range.start.character)
            || node_start.line > range.end.line
            || (node_start.line == range.end.line && node_start.character > range.end.character)
        {
            return;
        }
    }

    let kind = node.kind();

    // Skip structural nodes that don't represent actual tokens
    if is_structural_node(kind) {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                traverse(child, source, range_filter, out);
            }
        }
        return;
    }

    // Try to classify this node
    if let Some(token_type) = classify_node(node, kind, source) {
        // Skip wrapper keyword nodes that contain children of the same kind
        // (e.g., tree-sitter "transition" node wrapping "transition" keyword + "accept" identifier)
        if token_type == 7 && node.child_count() > 0 {
            // Recurse into children without emitting a token for this wrapper
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    traverse(child, source, range_filter, out);
                }
            }
            return;
        }

        let start = node.start_position();
        let line = start.row as u32;
        let char = start.column as u32;
        // length in characters (for ASCII, byte count = char count)
        let length = (node.end_byte() - node.start_byte()) as u32;

        // Apply range filter more precisely
        if let Some(range) = range_filter {
            if line < range.start.line
                || (line == range.start.line && char + length <= range.start.character)
                || line > range.end.line
                || (line == range.end.line && char >= range.end.character)
            {
                // Skip, but still traverse children for nested tokens
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        traverse(child, source, range_filter, out);
                    }
                }
                return;
            }
        }

        out.push(Token {
            line,
            char,
            length,
            token_type,
        });
    }

    // For nodes that didn't produce a token but have children, traverse them
    // (e.g., type_identifier is wrapped inside identifier, we already emitted
    // for type_identifier but might need to check if children should also be traversed)
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            traverse(child, source, range_filter, out);
        }
    }
}

/// Check if a node kind is a structural container, not a token.
fn is_structural_node(kind: &str) -> bool {
    matches!(
        kind,
        "source_file"
            | "top"
            | "field"
            | "parameter"
            | "control_body"
            | "control_body_element"
            | "parser_body"
            | "parser_body_element"
            | "table_element"
            | "action_item"
            | "expr"
            | "lval"
            | "fval"
            | "call"
            | "stmt"
            | "block_statement"
            | "conditional"
            | "for_statement"
            | "switch_statement"
            | "annotated_action"
            | "annotated_table"
            | "method"
    )
}

/// Classify a tree-sitter node into a semantic token type index.
fn classify_node(node: Node, kind: &str, _source: &str) -> Option<u32> {
    match kind {
        // Definition nodes: header, struct, enum -> namespace
        "header_definition" | "header_union_definition" | "struct_definition"
        | "enum_definition" => {
            // We want to highlight the name inside, not the whole definition.
            // Find the type_identifier child and emit for it.
            // But we traverse children separately, so don't emit for the definition itself.
            None
        }

        // State keyword in parser
        "state" => {
            // Tree-sitter produces both wrapper "state" nodes (with children) and
            // leaf "state" keyword nodes. Only the leaf is a real keyword token.
            if node.child_count() == 0 {
                Some(7) // keyword
            } else {
                None // structural wrapper
            }
        }

        // Type identifiers
        "type_identifier" => {
            // Check if this is inside a definition (definition position)
            let is_definition = is_inside_definition(node);
            if is_definition {
                Some(1) // type
            } else {
                // Type references are also "type"
                Some(1) // type
            }
        }

        // Method identifiers: parser/control/action names
        "method_identifier" => {
            let parent_kind = node.parent().map(|p| p.kind());
            match parent_kind {
                Some("parser_definition" | "control_definition" | "action" | "function_declaration" | "method" | "state") => {
                    Some(2) // function
                }
                Some("call" | "fval") => Some(2), // function call
                Some("action_item") => Some(2),   // action reference in table
                _ => {
                    // Check if it's a state name or other callable
                    Some(2) // default to function for method_identifier
                }
            }
        }

        // Regular identifiers
        "identifier" => {
            let parent_kind = node.parent().map(|p| p.kind());
            match parent_kind {
                Some("type_identifier") => None, // handled by type_identifier
                Some("method_identifier") => None, // handled by method_identifier
                Some("parameter") => Some(3),      // variable (parameter)
                Some("field") => Some(3),          // variable (field name)
                Some("variable_declaration") | Some("var_decl") => Some(3), // variable
                Some("const_definition") => Some(3), // variable (constant)
                Some("enum_definition") => Some(3), // enum member
                _ => Some(3),                       // variable
            }
        }

        // Numbers
        "number" | "decimal" | "hex" | "binary" | "octal" => Some(4),

        // String literals
        "string_literal" => Some(5),

        // Comments
        "comment" => Some(6),

        // Keywords
        _ => {
            if P4_KEYWORDS.contains(&kind) {
                Some(7) // keyword
            } else {
                None
            }
        }
    }
}

/// Check if a type_identifier node is inside a definition (its name is being defined).
fn is_inside_definition(node: Node) -> bool {
    let mut current = node;
    while let Some(parent) = current.parent() {
        let pkind = parent.kind();
        match pkind {
            "header_definition"
            | "header_union_definition"
            | "struct_definition"
            | "enum_definition"
            | "parser_definition"
            | "control_definition"
            | "extern_definition"
            | "table"
            | "annotated_table"
            | "typedef_definition"
            | "type_definition"
            | "value_set_declaration" => return true,
            // Don't cross expression boundaries
            "expr" | "lval" | "call" | "fval" | "stmt" | "block_statement" | "conditional"
            | "for_statement" | "switch_statement" | "state" | "control_body"
            | "control_body_element" | "parser_body" | "parser_body_element" => return false,
            _ => {}
        }
        current = parent;
    }
    false
}

fn ts_point_to_lsp(point: Point) -> Position {
    Position {
        line: point.row as u32,
        character: point.column as u32,
    }
}

/// Encode tokens using LSP delta encoding.
fn encode_tokens(tokens: &[Token]) -> SemanticTokens {
    let mut data = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for token in tokens {
        let delta_line = token.line.saturating_sub(prev_line);
        let delta_char = if delta_line == 0 {
            token.char.saturating_sub(prev_char)
        } else {
            token.char
        };

        data.push(SemanticToken {
            delta_line,
            delta_start: delta_char,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = token.line;
        prev_char = token.char;
    }

    SemanticTokens {
        result_id: None,
        data,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn parse_source(source: &str) -> tree_sitter::Tree {
        let mut parser = parser::new_parser().unwrap();
        parser.parse(source, None).expect("parse should succeed")
    }

    /// Helper: decode the delta-encoded tokens back to absolute positions for testing.
    fn decode_tokens(tokens: &SemanticTokens) -> Vec<(u32, u32, u32, u32)> {
        let mut result = Vec::new();
        let data = &tokens.data;
        let mut line = 0u32;
        let mut char = 0u32;
        let mut i = 0;
        while i < data.len() {
            line += data[i].delta_line;
            if data[i].delta_line == 0 {
                char += data[i].delta_start;
            } else {
                char = data[i].delta_start;
            }
            let length = data[i].length;
            let token_type = data[i].token_type;
            result.push((line, char, length, token_type));
            i += 1;
        }
        result
    }

    #[test]
    fn test_semantic_tokens_basic() {
        let source = "header ethernet_t {\n    bit<48> dstAddr;\n}";
        let tree = parse_source(source);
        let tokens = semantic_tokens_full(&tree, source);
        let decoded = decode_tokens(&tokens);

        // Expected tokens:
        // line 0: "header" (keyword=7), "ethernet_t" (type=1)
        // line 1: "bit" (keyword=7), "48" (number=4), "dstAddr" (variable=3)
        assert!(!decoded.is_empty(), "should produce tokens");

        // Check keyword "header" at line 0, char 0
        let header_token = decoded.iter().find(|(l, c, len, _)| *l == 0 && *c == 0 && *len == 6);
        assert!(
            header_token.is_some(),
            "should have 'header' token at (0,0), got: {:?}",
            decoded
        );
        assert_eq!(header_token.unwrap().3, 7, "'header' should be keyword(7)");

        // Check type "ethernet_t" at line 0
        let type_token = decoded.iter().find(|(l, _, len, ty)| *l == 0 && *len == 10 && *ty == 1);
        assert!(
            type_token.is_some(),
            "should have 'ethernet_t' type token at line 0, got: {:?}",
            decoded
        );

        // Check keyword "bit" at line 1
        let bit_token = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 4 && *len == 3 && *ty == 7);
        assert!(
            bit_token.is_some(),
            "should have 'bit' keyword token at line 1, got: {:?}",
            decoded
        );

        // Check number "48" at line 1
        let num_token = decoded.iter().find(|(l, _, len, ty)| *l == 1 && *len == 2 && *ty == 4);
        assert!(
            num_token.is_some(),
            "should have '48' number token at line 1, got: {:?}",
            decoded
        );

        // Check variable "dstAddr" at line 1
        let var_token = decoded.iter().find(|(l, _, len, ty)| *l == 1 && *len == 7 && *ty == 3);
        assert!(
            var_token.is_some(),
            "should have 'dstAddr' variable token at line 1, got: {:?}",
            decoded
        );
    }

    #[test]
    fn test_semantic_tokens_enum_and_parser() {
        let source = "enum Color { RED, GREEN }\nparser MyParser(packet_in pkt) {\n    state start {\n        transition accept;\n    }\n}";
        let tree = parse_source(source);
        let tokens = semantic_tokens_full(&tree, source);
        let decoded = decode_tokens(&tokens);

        // "enum" -> keyword(7)
        let enum_kw = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 0 && *len == 4 && *ty == 7);
        assert!(enum_kw.is_some(), "should have 'enum' keyword");

        // "Color" -> type(1)
        let color_ty = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 5 && *len == 5 && *ty == 1);
        assert!(color_ty.is_some(), "should have 'Color' type token, got: {:?}", decoded);

        // "RED", "GREEN" -> variable(3) [enum members]
        let red_var = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 13 && *len == 3 && *ty == 3);
        assert!(red_var.is_some(), "should have 'RED' variable token, got: {:?}", decoded);

        // "parser" -> keyword(7)
        let parser_kw = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 0 && *len == 6 && *ty == 7);
        assert!(parser_kw.is_some(), "should have 'parser' keyword");

        // "MyParser" -> function(2)
        let myparser_fn = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 7 && *len == 8 && *ty == 2);
        assert!(myparser_fn.is_some(), "should have 'MyParser' function token, got: {:?}", decoded);

        // "packet_in" -> keyword(7)
        let packet_in_kw = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 16 && *len == 9 && *ty == 7);
        assert!(packet_in_kw.is_some(), "should have 'packet_in' keyword");

        // "pkt" -> variable(3)
        let pkt_var = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 26 && *len == 3 && *ty == 3);
        assert!(pkt_var.is_some(), "should have 'pkt' variable token, got: {:?}", decoded);

        // "state" -> keyword(7)
        let state_kw = decoded.iter().find(|(l, c, len, ty)| *l == 2 && *c == 4 && *len == 5 && *ty == 7);
        assert!(state_kw.is_some(), "should have 'state' keyword");

        // "start" -> function(2) [state name uses method_identifier]
        let start_fn = decoded.iter().find(|(l, c, len, ty)| *l == 2 && *c == 10 && *len == 5 && *ty == 2);
        assert!(start_fn.is_some(), "should have 'start' function token, got: {:?}", decoded);
    }

    #[test]
    fn test_semantic_tokens_comment_and_string() {
        let source = "// line comment\nconst string X = \"hello\";\n/* block */";
        let tree = parse_source(source);
        let tokens = semantic_tokens_full(&tree, source);
        let decoded = decode_tokens(&tokens);

        // "// line comment" -> comment(6)
        let comment_token = decoded.iter().find(|(l, _c, _len, ty)| *l == 0 && *ty == 6);
        assert!(comment_token.is_some(), "should have comment token, got: {:?}", decoded);

        // "const" -> keyword(7)
        let const_kw = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 0 && *len == 5 && *ty == 7);
        assert!(const_kw.is_some(), "should have 'const' keyword");

        // "string" -> keyword(7)
        let string_kw = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 6 && *len == 6 && *ty == 7);
        assert!(string_kw.is_some(), "should have 'string' keyword");

        // "X" -> variable(3)
        let x_var = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 13 && *len == 1 && *ty == 3);
        assert!(x_var.is_some(), "should have 'X' variable token");

        // "\"hello\"" -> string(5)
        let string_token = decoded.iter().find(|(l, _c, _len, ty)| *l == 1 && *ty == 5);
        assert!(string_token.is_some(), "should have string literal token, got: {:?}", decoded);

        // "/* block */" -> comment(6)
        let block_comment = decoded.iter().find(|(l, _c, _len, ty)| *l == 2 && *ty == 6);
        assert!(block_comment.is_some(), "should have block comment token");
    }

    #[test]
    fn test_semantic_tokens_control_and_action() {
        let source = "control MyC(inout bit<8> x) {\n    action drop() {}\n    apply {}\n}";
        let tree = parse_source(source);
        let tokens = semantic_tokens_full(&tree, source);
        let decoded = decode_tokens(&tokens);

        // "control" -> keyword(7)
        let control_kw = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 0 && *len == 7 && *ty == 7);
        assert!(control_kw.is_some(), "should have 'control' keyword");

        // "MyC" -> function(2)
        let myc_fn = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 8 && *len == 3 && *ty == 2);
        assert!(myc_fn.is_some(), "should have 'MyC' function token, got: {:?}", decoded);

        // "inout" -> keyword(7)
        let inout_kw = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 12 && *len == 5 && *ty == 7);
        assert!(inout_kw.is_some(), "should have 'inout' keyword");

        // "bit" -> keyword(7)
        let bit_kw = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 18 && *len == 3 && *ty == 7);
        assert!(bit_kw.is_some(), "should have 'bit' keyword");

        // "8" -> number(4)
        let num_token = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 22 && *len == 1 && *ty == 4);
        assert!(num_token.is_some(), "should have '8' number token");

        // "x" -> variable(3)
        let x_var = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 25 && *len == 1 && *ty == 3);
        assert!(x_var.is_some(), "should have 'x' variable token");

        // "action" -> keyword(7)
        let action_kw = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 4 && *len == 6 && *ty == 7);
        assert!(action_kw.is_some(), "should have 'action' keyword");

        // "drop" -> function(2)
        let drop_fn = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 11 && *len == 4 && *ty == 2);
        assert!(drop_fn.is_some(), "should have 'drop' function token");

        // "apply" -> keyword(7)
        let apply_kw = decoded.iter().find(|(l, c, len, ty)| *l == 2 && *c == 4 && *len == 5 && *ty == 7);
        assert!(apply_kw.is_some(), "should have 'apply' keyword");
    }

    #[test]
    fn test_semantic_tokens_range_filter() {
        let source = "header ethernet_t {\n    bit<48> dstAddr;\n}";
        let tree = parse_source(source);

        // Range covering only line 1
        let range = Range {
            start: Position { line: 1, character: 0 },
            end: Position { line: 1, character: 30 },
        };
        let tokens = semantic_tokens_range(&tree, source, range);
        let decoded = decode_tokens(&tokens);

        // Should only contain tokens from line 1
        assert!(
            decoded.iter().all(|(l, _, _, _)| *l == 1),
            "range tokens should all be on line 1, got: {:?}",
            decoded
        );

        // Should have bit keyword, number, and variable
        let bit_kw = decoded.iter().find(|(_, c, len, ty)| *c == 4 && *len == 3 && *ty == 7);
        assert!(bit_kw.is_some(), "should have 'bit' keyword in range");
    }

    #[test]
    fn test_semantic_tokens_extern_and_method() {
        let source = "extern MyExtern {\n    void method1(in bit<8> a);\n}";
        let tree = parse_source(source);
        let tokens = semantic_tokens_full(&tree, source);
        let decoded = decode_tokens(&tokens);

        // "extern" -> keyword(7)
        let extern_kw = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 0 && *len == 6 && *ty == 7);
        assert!(extern_kw.is_some(), "should have 'extern' keyword");

        // "MyExtern" -> type(1)
        let myextern_ty = decoded.iter().find(|(l, c, len, ty)| *l == 0 && *c == 7 && *len == 8 && *ty == 1);
        assert!(myextern_ty.is_some(), "should have 'MyExtern' type token, got: {:?}", decoded);

        // "void" -> keyword(7)
        let void_kw = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 4 && *len == 4 && *ty == 7);
        assert!(void_kw.is_some(), "should have 'void' keyword");

        // "method1" -> function(2)
        let method1_fn = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 9 && *len == 7 && *ty == 2);
        assert!(method1_fn.is_some(), "should have 'method1' function token, got: {:?}", decoded);

        // "in" -> keyword(7)
        let in_kw = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 17 && *len == 2 && *ty == 7);
        assert!(in_kw.is_some(), "should have 'in' keyword");

        // "bit" -> keyword(7)
        let bit_kw = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 20 && *len == 3 && *ty == 7);
        assert!(bit_kw.is_some(), "should have 'bit' keyword");

        // "8" -> number(4)
        let num_token = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 24 && *len == 1 && *ty == 4);
        assert!(num_token.is_some(), "should have '8' number token");

        // "a" -> variable(3)
        let a_var = decoded.iter().find(|(l, c, len, ty)| *l == 1 && *c == 27 && *len == 1 && *ty == 3);
        assert!(a_var.is_some(), "should have 'a' variable token");
    }
}
