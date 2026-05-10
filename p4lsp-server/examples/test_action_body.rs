use p4lsp_server::parser;

fn main() {
    let code = r#"
control MyControl(inout ethernet_t eth) {
    action forward(bit<9> port) {
        bit<16> action_local = 1;
    }
    apply {}
}
"#;
    let mut parser = parser::new_parser().unwrap();
    let tree = parser.parse(code, None).unwrap();
    fn dump(node: tree_sitter::Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = &source[node.start_byte()..node.end_byte()];
        let display_text = if text.len() > 60 { &text[..60] } else { text };
        println!("{}{} [{},{}..{},{}]: {:?}", 
            indent, node.kind(), 
            node.start_position().row, node.start_position().column,
            node.end_position().row, node.end_position().column,
            display_text);
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                dump(child, source, depth + 1);
            }
        }
    }
    dump(tree.root_node(), code, 0);
}
