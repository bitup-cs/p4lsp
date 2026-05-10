use p4lsp_server::parser;

fn main() {
    let source = r#"struct B { bit<32> x; bit<16> y; }
struct A { B b; }"#;
    let mut p = parser::new_parser().unwrap();
    let tree = p.parse(source, None).expect("parse");
    fn print_tree(node: tree_sitter::Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = &source[node.start_byte()..node.end_byte().min(source.len())];
        println!("{}{}: row{} col{} = {:?}", indent, node.kind(), node.start_position().row, node.start_position().column, text);
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                print_tree(child, source, depth + 1);
            }
        }
    }
    print_tree(tree.root_node(), source, 0);
}
