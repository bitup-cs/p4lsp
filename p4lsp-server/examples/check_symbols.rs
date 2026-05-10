use p4lsp_server::parser;
use p4lsp_server::workspace::WorkspaceIndex;
use tower_lsp::lsp_types::Url;

fn main() {
    let source = r#"
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
    let mut p = parser::new_parser().unwrap();
    let tree = p.parse(source, None).unwrap();
    let index = WorkspaceIndex::new();
    let uri = Url::parse("file:///test.p4").unwrap();
    index.index_document(&uri, &tree, source);
    let fi = index.files.get(&uri).unwrap();
    for (i, s) in fi.value().symbols.iter().enumerate() {
        println!("{}: {} ({:?})", i, s.name, s.kind);
    }
    println!("Total: {}", fi.value().symbols.len());
}
