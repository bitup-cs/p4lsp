use std::time::Instant;
use p4lsp_server::document::Document;
use p4lsp_server::parser;
use tower_lsp::lsp_types::{TextDocumentContentChangeEvent, Position, Range};

fn main() {
    let source = r#"
header ethernet_t {
    bit<48> dst_addr;
    bit<48> src_addr;
    bit<16> ether_type;
}

struct headers_t {
    ethernet_t ethernet;
}

parser Parser(packet_in pkt, out headers_t hdr) {
    state start {
        pkt.extract(hdr.ethernet);
        transition accept;
    }
}

control Ingress(inout headers_t hdr, inout standard_metadata_t meta) {
    action drop() {
        mark_to_drop(meta);
    }
    table forward {
        key = { hdr.ethernet.dst_addr: exact; }
        actions = { drop; }
        size = 1024;
    }
    apply {
        forward.apply();
    }
}

control Deparser(packet_out pkt, in headers_t hdr) {
    apply {
        pkt.emit(hdr.ethernet);
    }
}

package Top(Parser(), Ingress(), Deparser());
"#;

    let mut parser = parser::new_parser().unwrap();
    let start = Instant::now();
    let _tree = parser.parse(source, None).unwrap();
    let init_ms = start.elapsed().as_millis();
    println!("Initial parse: {} ms", init_ms);

    let uri = tower_lsp::lsp_types::Url::parse("file:///test.p4").unwrap();
    let mut doc = Document::new(uri.clone(), source.to_string(), &mut parser).unwrap();

    let change = TextDocumentContentChangeEvent {
        range: Some(Range {
            start: Position { line: 2, character: 4 },
            end: Position { line: 2, character: 12 },
        }),
        range_length: None,
        text: "bit<64>".to_string(),
    };

    let start = Instant::now();
    doc.apply_changes(vec![change]).unwrap();
    doc.reparse().unwrap();
    let reparse_ms = start.elapsed().as_micros();
    println!("Incremental reparse: {} μs", reparse_ms);

    let start = Instant::now();
    let _symbols = p4lsp_server::index::document_symbols(doc.tree.root_node(), &doc.text());
    let index_ms = start.elapsed().as_micros();
    println!("Document symbol index: {} μs", index_ms);

    let start = Instant::now();
    let _diags = p4lsp_server::diagnostics::tree_diagnostics(&doc.tree, &doc.text(), &doc.uri);
    let diag_ms = start.elapsed().as_micros();
    println!("Diagnostics: {} μs", diag_ms);

    println!("\nAll benchmarks complete.");
}
