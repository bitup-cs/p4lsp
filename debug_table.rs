use tree_sitter::{Node, Parser, Point, Tree};

fn main() {
    let source = r#"control MyCtl(inout bit<8> x) {
    action do_drop() {}
    table my_tbl {
        key = { x : exact; }
        actions = { do_drop; }
    }
    apply { my_tbl.apply(); }
}"#;

    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_p4::language().into()).unwrap();
    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();

    // Print the node at position (3, 10) - "my_tbl"
    let point = Point { row: 3, column: 10 };
    if let Some(node) = root.descendant_for_point_range(point, point) {
        print_node(node, source, 0);
    }
}

fn print_node(node: Node, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = &source[node.start_byte()..node.end_byte()];
    println!(
        "{}{} [{}:{} - {}:{}] = {:?}",
        indent,
        node.kind(),
        node.start_position().row,
        node.start_position().column,
        node.end_position().row,
        node.end_position().column,
        text
    );
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            print_node(child, source, depth + 1);
        }
    }
}
