use std::collections::HashMap;
use tower_lsp::lsp_types::{Position, Range, TextEdit, Url};
use tree_sitter::{Node, Tree};

use crate::workspace::WorkspaceIndex;

/// Result of collecting rename targets.
#[derive(Debug)]
pub struct RenameTargets {
    /// Whether this is a local symbol (should only rename in current file)
    pub is_local: bool,
    /// The original symbol name
    pub name: String,
    /// Ranges per file to rename
    pub edits: HashMap<Url, Vec<Range>>,
}

/// Determine if a symbol at a position is local or global, and prepare rename targets.
pub fn prepare_rename(
    uri: &Url,
    pos: Position,
    tree: &Tree,
    source: &str,
    index: &WorkspaceIndex,
) -> Option<RenameTargets> {
    let word = word_at_pos(source, pos)?;

    // Check if it's a local symbol
    let scope = index.scope_at(uri, pos, tree, source);
    let is_local = scope.params.iter().any(|(n, _)| n == &word)
        || scope.locals.iter().any(|(n, _)| n == &word);

    let mut edits: HashMap<Url, Vec<Range>> = HashMap::new();

    if is_local {
        // Local: rename in current file only, bounded by scope
        let boundary = find_scope_boundary(tree, pos);
        let ranges = if let Some(boundary_node) = boundary {
            collect_matching_in_scope(boundary_node, source, &word)
        } else {
            collect_matching_nodes(tree.root_node(), source, &word)
        };
        if !ranges.is_empty() {
            edits.insert(uri.clone(), ranges);
        }
    } else {
        // Global: rename across ALL indexed files
        let current_ranges = collect_matching_nodes(tree.root_node(), source, &word);
        if !current_ranges.is_empty() {
            edits.insert(uri.clone(), current_ranges);
        }

        // Search every other indexed file for references
        for entry in index.files.iter() {
            let file_uri = entry.key().clone();
            if file_uri == *uri {
                continue;
            }
            let fi = entry.value();
            if let Some(ref file_tree) = fi.tree {
                let ranges = collect_matching_nodes(file_tree.root_node(), &fi.source, &word);
                if !ranges.is_empty() {
                    edits.entry(file_uri).or_insert(ranges);
                }
            }
        }
    }

    if edits.is_empty() {
        return None;
    }

    Some(RenameTargets {
        is_local,
        name: word,
        edits,
    })
}

/// Build a WorkspaceEdit from rename targets and a new name.
pub fn build_workspace_edit(targets: RenameTargets, new_name: String) -> tower_lsp::lsp_types::WorkspaceEdit {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for (file_uri, ranges) in targets.edits {
        let text_edits: Vec<TextEdit> = ranges
            .into_iter()
            .map(|range| TextEdit {
                range,
                new_text: new_name.clone(),
            })
            .collect();
        changes.insert(file_uri, text_edits);
    }
    tower_lsp::lsp_types::WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

// ---------------------------------------------------------------------------
// AST helpers
// ---------------------------------------------------------------------------

/// Collect all identifier / type_identifier / method_identifier nodes matching `name`.
fn collect_matching_nodes(node: Node, source: &str, name: &str) -> Vec<Range> {
    let mut ranges = Vec::new();
    collect_matching_recursive(node, source, name, &mut ranges);
    ranges.sort_by_key(|r| (r.start.line, r.start.character));
    ranges.dedup_by(|a, b| a.start == b.start && a.end == b.end);
    ranges
}

fn collect_matching_recursive(node: Node, source: &str, name: &str, out: &mut Vec<Range>) {
    let kind = node.kind();
    if kind == "identifier" || kind == "type_identifier" || kind == "method_identifier" {
        let text = &source[node.start_byte()..node.end_byte()];
        if text == name {
            out.push(ts_range_to_lsp(node));
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_matching_recursive(child, source, name, out);
        }
    }
}

/// Collect matching nodes only within a scope boundary (for local symbols).
fn collect_matching_in_scope(boundary: Node, source: &str, name: &str) -> Vec<Range> {
    let mut ranges = Vec::new();
    collect_matching_recursive(boundary, source, name, &mut ranges);
    ranges.sort_by_key(|r| (r.start.line, r.start.character));
    ranges.dedup_by(|a, b| a.start == b.start && a.end == b.end);
    ranges
}

/// Find the nearest scope boundary node for a local symbol.
fn find_scope_boundary(tree: &Tree, pos: Position) -> Option<Node<'_>> {
    let point = tree_sitter::Point {
        row: pos.line as usize,
        column: pos.character as usize,
    };
    let node = tree.root_node().descendant_for_point_range(point, point)?;

    let mut current = Some(node);
    while let Some(n) = current {
        match n.kind() {
            "action"
            | "annotated_action"
            | "control_definition"
            | "parser_definition"
            | "function_declaration"
            | "block_statement"
            | "state"
            | "for_statement"
            | "conditional" => return Some(n),
            _ => {}
        }
        current = n.parent();
    }
    None
}

fn ts_range_to_lsp(node: Node) -> Range {
    Range {
        start: Position {
            line: node.start_position().row as u32,
            character: node.start_position().column as u32,
        },
        end: Position {
            line: node.end_position().row as u32,
            character: node.end_position().column as u32,
        },
    }
}

fn word_at_pos(source: &str, pos: Position) -> Option<String> {
    let line = source.lines().nth(pos.line as usize)?;
    let target_cu = pos.character as usize;

    let mut cu = 0;
    let mut cursor_byte = None;
    for (byte_idx, c) in line.char_indices() {
        if cu >= target_cu {
            cursor_byte = Some(byte_idx);
            break;
        }
        cu += c.len_utf16();
    }
    let cursor_byte = cursor_byte?;

    let cursor_char = line[cursor_byte..].chars().next()?;
    if !is_word_char(cursor_char) {
        return None;
    }

    let mut start = cursor_byte;
    while start > 0 {
        let prev = line[..start].chars().next_back()?;
        if !is_word_char(prev) {
            break;
        }
        start -= prev.len_utf8();
    }

    let mut end = cursor_byte + cursor_char.len_utf8();
    while end < line.len() {
        let next = line[end..].chars().next()?;
        if !is_word_char(next) {
            break;
        }
        end += next.len_utf8();
    }

    Some(line[start..end].to_string())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use tower_lsp::lsp_types::{Position, Url};

    fn parse_p4(source: &str) -> Tree {
        let mut p = parser::new_parser().expect("new_parser should succeed");
        p.parse(source, None).expect("parse should succeed")
    }

    /// Test 1: Rename a local variable inside an action.
    /// The variable `action_local` appears in its declaration and in an assignment.
    /// Renaming it should only affect occurrences inside the same action.
    #[test]
    fn test_rename_local_variable() {
        let source = r#"
control MyControl() {
    action drop() {
        bit<16> action_local = 1;
        action_local = 2;
    }
    action forward() {
        bit<16> action_local = 3;
    }
    apply {}
}
"#;
        let tree = parse_p4(source);
        let index = WorkspaceIndex::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        index.index_document(&uri, &tree, source);

        // Cursor on the first action_local declaration inside `drop`
        let pos = Position { line: 3, character: 16 };
        let targets = prepare_rename(&uri, pos, &tree, source, &index).expect("should find rename targets");

        assert!(targets.is_local, "action_local should be a local symbol");
        assert_eq!(targets.name, "action_local");

        // Should only have edits in the current file
        let ranges = targets.edits.get(&uri).expect("should have edits in current file");

        // Expect 2 occurrences: declaration + assignment, both inside `drop` action
        assert_eq!(ranges.len(), 2, "should rename 2 occurrences of action_local in drop action");

        // Verify both are on lines 3 and 4 (0-based)
        let lines: Vec<u32> = ranges.iter().map(|r| r.start.line).collect();
        assert!(lines.contains(&3), "should include declaration on line 3");
        assert!(lines.contains(&4), "should include usage on line 4");
    }

    /// Test 2: Rename a control parameter.
    /// The parameter `eth` appears in the parameter list and inside apply.
    #[test]
    fn test_rename_control_parameter() {
        let source = r#"
header ethernet_t { bit<48> dst_addr; }
control MyControl(inout ethernet_t eth) {
    apply {
        eth.dst_addr = 0;
    }
}
"#;
        let tree = parse_p4(source);
        let index = WorkspaceIndex::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        index.index_document(&uri, &tree, source);

        // Cursor on `eth` in the parameter list
        let pos = Position { line: 2, character: 36 };
        let targets = prepare_rename(&uri, pos, &tree, source, &index).expect("should find rename targets");

        assert!(targets.is_local, "control parameter eth should be a local symbol");
        assert_eq!(targets.name, "eth");

        let ranges = targets.edits.get(&uri).unwrap();
        // Expect 2: parameter declaration + field access usage
        assert_eq!(ranges.len(), 2, "should rename 2 occurrences of eth");
    }

    /// Test 3: Rename a global header definition.
    /// The header `ethernet_t` appears in its definition and as a type in a parameter.
    /// Since it's global, the rename should span all files (here, just one file).
    #[test]
    fn test_rename_global_header() {
        let source = r#"
header ethernet_t { bit<48> dst_addr; }
control MyControl(inout ethernet_t eth) {
    apply {}
}
"#;
        let tree = parse_p4(source);
        let index = WorkspaceIndex::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        index.index_document(&uri, &tree, source);

        // Cursor on `ethernet_t` in the header definition
        let pos = Position { line: 1, character: 10 };
        let targets = prepare_rename(&uri, pos, &tree, source, &index).expect("should find rename targets");

        assert!(!targets.is_local, "ethernet_t should be a global symbol");
        assert_eq!(targets.name, "ethernet_t");

        let ranges = targets.edits.get(&uri).unwrap();
        // Expect 2: header definition + type usage in parameter
        assert_eq!(ranges.len(), 2, "should rename 2 occurrences of ethernet_t");
    }

    /// Test 4: Rename a global symbol across multiple files.
    /// Two files define a control with the same name? No — we index both, and the symbol
    /// `shared_t` is defined in file A and used in file B. Renaming should edit both files.
    #[test]
    fn test_rename_cross_file() {
        let source_a = r#"
header shared_t { bit<32> f; }
"#;
        let source_b = r#"
control C(inout shared_t s) {
    apply {}
}
"#;
        let tree_a = parse_p4(source_a);
        let tree_b = parse_p4(source_b);
        let index = WorkspaceIndex::new();
        let uri_a = Url::parse("file:///a.p4").unwrap();
        let uri_b = Url::parse("file:///b.p4").unwrap();
        index.index_document(&uri_a, &tree_a, source_a);
        index.index_document(&uri_b, &tree_b, source_b);

        // Rename from file A, cursor on `shared_t` definition
        let pos = Position { line: 1, character: 10 };
        let targets = prepare_rename(&uri_a, pos, &tree_a, source_a, &index).expect("should find rename targets");

        assert!(!targets.is_local, "shared_t should be global");
        assert_eq!(targets.name, "shared_t");

        // Should have edits in both files
        assert!(targets.edits.contains_key(&uri_a), "should have edits in file A");
        assert!(targets.edits.contains_key(&uri_b), "should have edits in file B");

        let ranges_a = targets.edits.get(&uri_a).unwrap();
        let ranges_b = targets.edits.get(&uri_b).unwrap();

        assert_eq!(ranges_a.len(), 1, "file A has 1 occurrence (definition)");
        assert_eq!(ranges_b.len(), 1, "file B has 1 occurrence (type usage)");
    }

    /// Test 5: WorkspaceEdit generation.
    #[test]
    fn test_build_workspace_edit() {
        let mut edits = HashMap::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        edits.insert(
            uri.clone(),
            vec![Range {
                start: Position { line: 1, character: 10 },
                end: Position { line: 1, character: 20 },
            }],
        );

        let targets = RenameTargets {
            is_local: false,
            name: "old_name".to_string(),
            edits,
        };

        let workspace_edit = build_workspace_edit(targets, "new_name".to_string());
        let changes = workspace_edit.changes.expect("should have changes");
        let file_edits = changes.get(&uri).expect("should have file edits");
        assert_eq!(file_edits.len(), 1);
        assert_eq!(file_edits[0].new_text, "new_name");
        assert_eq!(file_edits[0].range.start.line, 1);
    }

    /// Test 6: No rename target when cursor is on empty space or non-word.
    #[test]
    fn test_rename_no_target() {
        let source = "header H { bit<32> f; }";
        let tree = parse_p4(source);
        let index = WorkspaceIndex::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        index.index_document(&uri, &tree, source);

        // Cursor on a brace `{`
        let pos = Position { line: 0, character: 9 };
        let targets = prepare_rename(&uri, pos, &tree, source, &index);
        assert!(targets.is_none(), "should not find rename target on non-word character");
    }

    /// Test 7: Rename an action (method_identifier) and its references in table actions.
    #[test]
    fn test_rename_action() {
        let source = r#"
control MyControl() {
    action drop() {}
    action forward(bit<9> port) {}
    table ipv4_table {
        key = { }
        actions = { drop; forward; }
    }
    apply {
        ipv4_table.apply();
    }
}
"#;
        let tree = parse_p4(source);
        let index = WorkspaceIndex::new();
        let uri = Url::parse("file:///test.p4").unwrap();
        index.index_document(&uri, &tree, source);

        // Cursor on `drop` action definition (character 11 = 'd')
        let pos = Position { line: 2, character: 11 };
        let targets = prepare_rename(&uri, pos, &tree, source, &index).expect("should find rename targets");

        assert!(!targets.is_local, "drop action should be global (indexed as top-level action)");
        assert_eq!(targets.name, "drop");

        let ranges = targets.edits.get(&uri).unwrap();
        // Expect: action definition + reference in table actions list
        assert_eq!(ranges.len(), 2, "should rename drop definition and table reference");
    }
}
