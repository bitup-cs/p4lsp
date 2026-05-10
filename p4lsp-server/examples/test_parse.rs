use p4lsp_server::parser;

fn main() {
    let code = r#"
header ethernet_t {
    bit<48> dst_addr;
}

struct metadata_t {
    bit<32> value;
}

enum Color { RED, GREEN, BLUE }

parser MyParser(packet_in pkt) {
    state start {
        transition accept;
    }
}

control MyControl(inout ethernet_t eth) {
    action drop() {}
    action forward(bit<9> port) {}
    table ipv4_table {
        key = { eth.dst_addr : exact; }
        actions = { drop; forward; }
    }
    apply {
        ipv4_table.apply();
    }
}

extern MyExtern {
    void method1();
}
"#;
    let mut parser = parser::new_parser().unwrap();
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();
    println!("Root kind: {}", root.kind());
    fn dump(node: tree_sitter::Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = &source[node.start_byte()..node.end_byte()];
        let display_text = if text.len() > 40 { &text[..40] } else { text };
        println!("{}{} [{}..{}]: {:?}", indent, node.kind(), node.start_byte(), node.end_byte(), display_text);
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                dump(child, source, depth + 1);
            }
        }
    }
    dump(root, code, 0);
}
