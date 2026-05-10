fn main() {
    let source = r#"control MyCtl(inout bit<8> x) {
    action do_drop() {}
    table my_tbl {
        key = { x : exact; }
        actions = { do_drop; }
    }
    apply { my_tbl.apply(); }
}"#;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&p4lsp_server::parser::language()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();
    let point = tree_sitter::Point { row: 3, column: 10 };
    if let Some(node) = root.descendant_for_point_range(point, point) {
        println!("node kind: {}", node.kind());
        println!("node text: {}", &source[node.start_byte()..node.end_byte()]);
        if let Some(parent) = node.parent() {
            println!("parent kind: {}", parent.kind());
        }
    }
}
