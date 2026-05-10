use p4lsp_server::parser;
use std::fs;

fn main() {
    let source = fs::read_to_string(std::env::args().nth(1).unwrap_or("test.p4".to_string())).unwrap();
    let mut parser = parser::new_parser().unwrap();
    let tree = parser.parse(&source, None).expect("parse");
    
    fn walk_errors(node: tree_sitter::Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        if node.is_error() {
            println!("{}ERROR: '{}' at line {} col {}-line {} col {}", 
                indent,
                &source[node.start_byte()..node.end_byte()],
                node.start_position().row, node.start_position().column,
                node.end_position().row, node.end_position().column
            );
        }
        if node.is_missing() {
            println!("{}MISSING: '{}' at line {} col {}", 
                indent,
                node.kind(),
                node.start_position().row, node.start_position().column
            );
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                walk_errors(child, source, depth + 1);
            }
        }
    }
    
    println!("Root node kind: {}", tree.root_node().kind());
    walk_errors(tree.root_node(), &source, 0);
}
