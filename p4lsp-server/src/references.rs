use tree_sitter::Node;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

/// 递归收集 AST 中所有引用指定符号的节点位置。
/// 遍历当前文件的 AST，匹配 identifier / type_identifier / method_identifier 节点，
/// 将文本等于 `word` 的节点收集为 Location。
pub fn collect_reference_nodes(
    node: Node,
    source: &str,
    word: &str,
    uri: &Url,
    out: &mut Vec<Location>,
) {
    let kind = node.kind();
    if kind == "identifier" || kind == "type_identifier" || kind == "method_identifier" {
        if crate::hover::node_text(node, source) == word {
            let sp = node.start_position();
            let ep = node.end_position();
            out.push(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: sp.row as u32,
                        character: sp.column as u32,
                    },
                    end: Position {
                        line: ep.row as u32,
                        character: ep.column as u32,
                    },
                },
            });
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_reference_nodes(child, source, word, uri, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use tower_lsp::lsp_types::Url;

    fn parse_p4(source: &str) -> tree_sitter::Tree {
        let mut p = parser::new_parser().expect("new_parser should succeed");
        p.parse(source, None).expect("parse should succeed")
    }

    #[test]
    fn test_collect_reference_nodes_basic() {
        let source = r#"
header ethernet_t {
    bit<48> dst_addr;
}

struct metadata_t {
    bit<32> value;
}

parser MyParser(packet_in pkt) {
    state start {
        ethernet_t eth;
        transition accept;
    }
}

control MyControl(inout ethernet_t eth) {
    action drop() {}
    apply {
        eth.dst_addr = 0;
        drop();
    }
}
"#;
        let tree = parse_p4(source);
        let uri = Url::parse("file:///test.p4").unwrap();
        let mut locations = Vec::new();
        collect_reference_nodes(tree.root_node(), source, "ethernet_t", &uri, &mut locations);

        // ethernet_t 出现在：header 定义、parser 局部变量声明、control 参数类型
        assert!(
            locations.len() >= 3,
            "expected at least 3 references to ethernet_t, got {}",
            locations.len()
        );

        // 验证每个返回的 location 的 range 都在有效范围内
        for loc in &locations {
            assert_eq!(loc.uri, uri);
            assert!(loc.range.start.line <= loc.range.end.line);
        }
    }

    #[test]
    fn test_collect_reference_nodes_no_match() {
        let source = r#"header h { bit<8> f; }"#;
        let tree = parse_p4(source);
        let uri = Url::parse("file:///test.p4").unwrap();
        let mut locations = Vec::new();
        collect_reference_nodes(tree.root_node(), source, "nonexistent", &uri, &mut locations);
        assert!(locations.is_empty());
    }

    #[test]
    fn test_collect_reference_nodes_method_identifier() {
        let source = r#"
extern Checksum16 {
    void initialize();
    void update<T>(in T data);
}

control c() {
    Checksum16 ck;
    apply {
        ck.initialize();
        ck.update(1);
    }
}
"#;
        let tree = parse_p4(source);
        let uri = Url::parse("file:///test.p4").unwrap();
        let mut locations = Vec::new();
        collect_reference_nodes(tree.root_node(), source, "initialize", &uri, &mut locations);

        // initialize 出现在 extern 定义和 apply 中的调用
        assert!(
            locations.len() >= 2,
            "expected at least 2 references to initialize, got {}",
            locations.len()
        );

        let mut locations2 = Vec::new();
        collect_reference_nodes(tree.root_node(), source, "ck", &uri, &mut locations2);
        // ck 出现在变量声明和两次调用
        assert!(
            locations2.len() >= 3,
            "expected at least 3 references to ck, got {}",
            locations2.len()
        );
    }
}
