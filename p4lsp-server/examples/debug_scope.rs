use p4lsp_server::parser;

fn main() {
    // Test 1: control apply block with if
    let source1 = r#"control C() {
    apply {
        bit<32> outer = 0;
        if (true) {
            bit<16> inner = 1;
            inner;
        }
        outer;
    }
}"#;
    let mut p = parser::new_parser().unwrap();
    let tree = p.parse(source1, None).expect("parse");
    
    fn print_tree(node: tree_sitter::Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = &source[node.start_byte()..node.end_byte().min(source.len())];
        let short = if text.len() > 40 { &text[..40] } else { text };
        println!("{}{}: r{} c{} = {:?}", indent, node.kind(), node.start_position().row, node.start_position().column, short);
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                print_tree(child, source, depth + 1);
            }
        }
    }
    
    println!("=== Control apply block AST ===");
    print_tree(tree.root_node(), source1, 0);
    
    // Test 2: parser state
    let source2 = r#"parser P(packet_in pkt) {
    state start {
        bit<32> state_local = 0;
        state_local;
        transition accept;
    }
}"#;
    let tree2 = p.parse(source2, None).expect("parse");
    println!("\n=== Parser state AST ===");
    print_tree(tree2.root_node(), source2, 0);
}
