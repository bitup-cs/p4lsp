use ropey::Rope;
use tower_lsp::lsp_types::{TextDocumentContentChangeEvent, Url};
use tree_sitter::{Parser, Tree};

/// 一个打开的 P4 文档，内含复用的 Parser。
pub struct Document {
    #[allow(dead_code)]
    pub uri: Url,
    pub rope: Rope,
    pub tree: Tree,
    pub parser: Parser,
}

impl Document {
    pub fn new(uri: Url, text: String, parser: &mut Parser) -> Result<Self, String> {
        let tree = parser
            .parse(&text, None)
            .ok_or_else(|| "initial parse failed".to_string())?;
        let mut new_parser = Parser::new();
        new_parser.set_language(&parser.language().unwrap()).map_err(|e| format!("failed to clone parser language: {:?}", e))?;
        Ok(Self {
            uri,
            rope: Rope::from_str(&text),
            tree,
            parser: new_parser,
        })
    }

    /// 应用增量文本变更并更新 tree。返回 Err 时 tree 可能已部分损坏，调用方应回退。
    pub fn apply_changes(
        &mut self,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Result<(), String> {
        for change in changes {
            if let Some(range) = change.range {
                let start_idx = self.position_to_idx(range.start);
                let end_idx = self.position_to_idx(range.end);
                let start_byte = self.rope.char_to_byte(start_idx);
                let old_end_byte = self.rope.char_to_byte(end_idx);
                let new_end_byte = start_byte + change.text.len();

                self.rope.remove(start_idx..end_idx);
                self.rope.insert(start_idx, &change.text);

                let new_end_position =
                    ts_end_point(&self.rope, start_idx + change.text.chars().count());

                let input = EditInput {
                    start_byte,
                    old_end_byte,
                    new_end_byte,
                    start_position: ts_point(range.start),
                    old_end_position: ts_point(range.end),
                    new_end_position,
                };

                self.tree.edit(&input.into_tree_sitter());
            } else {
                // 全量替换
                self.rope = Rope::from_str(&change.text);
                self.tree = self
                    .parser
                    .parse(&change.text, None)
                    .ok_or_else(|| "full reparse failed".to_string())?;
            }
        }
        Ok(())
    }

    pub fn reparse(&mut self) -> Result<(), String> {
        let text = self.text();
        self.tree = self
            .parser
            .parse(&text, Some(&self.tree))
            .ok_or_else(|| "reparse failed".to_string())?;
        Ok(())
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// LSP Position → rope char index（正确处理 UTF-16 code units → char 转换）
    fn position_to_idx(&self, pos: tower_lsp::lsp_types::Position) -> usize {
        let line = pos.line as usize;
        let line_start = self.rope.line_to_char(line);
        let mut cu = 0;
        let mut chars = 0;
        for c in self.rope.slice(line_start..).chars() {
            if cu >= pos.character as usize {
                break;
            }
            cu += c.len_utf16();
            chars += 1;
        }
        line_start + chars
    }
}

/// 适配 tree-sitter InputEdit 的中间结构。
struct EditInput {
    start_byte: usize,
    old_end_byte: usize,
    new_end_byte: usize,
    start_position: tree_sitter::Point,
    old_end_position: tree_sitter::Point,
    new_end_position: tree_sitter::Point,
}

impl EditInput {
    fn into_tree_sitter(self) -> tree_sitter::InputEdit {
        tree_sitter::InputEdit {
            start_byte: self.start_byte,
            old_end_byte: self.old_end_byte,
            new_end_byte: self.new_end_byte,
            start_position: self.start_position,
            old_end_position: self.old_end_position,
            new_end_position: self.new_end_position,
        }
    }
}

fn ts_point(pos: tower_lsp::lsp_types::Position) -> tree_sitter::Point {
    tree_sitter::Point {
        row: pos.line as usize,
        column: pos.character as usize,
    }
}

/// rope char index → tree-sitter Point（column 为 UTF-16 code units，与 LSP 对齐）
fn ts_end_point(rope: &Rope, char_idx: usize) -> tree_sitter::Point {
    let line = rope.char_to_line(char_idx);
    let line_start = rope.line_to_char(line);
    let slice = rope.slice(line_start..char_idx);
    let col = slice.chars().map(|c| c.len_utf16()).sum::<usize>();
    tree_sitter::Point { row: line, column: col }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range};

    fn new_doc(text: &str) -> Document {
        let mut parser = crate::parser::new_parser().unwrap();
        Document::new(
            Url::parse("file:///test.p4").unwrap(),
            text.to_string(),
            &mut parser,
        )
        .unwrap()
    }

    /// 测试 1：UTF-8 中文字符增量同步
    #[test]
    fn test_utf8_incremental_sync() {
        let source = r#"// 你好世界
header H { bit<32> f; }"#;
        let mut doc = new_doc(source);

        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position { line: 1, character: 11 },
                end: Position { line: 1, character: 11 },
            }),
            range_length: None,
            text: "var".to_string(),
        };

        doc.apply_changes(vec![change]).unwrap();
        assert!(doc.text().contains("varbit"));
        doc.reparse().unwrap();
        assert_eq!(doc.tree.root_node().kind(), "source_file");
    }

    /// 测试 2：替换含中文注释的整行
    #[test]
    fn test_utf8_line_replacement() {
        let source = r#"// 这是中文注释
header H { bit<32> f; }"#;
        let mut doc = new_doc(source);

        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 1, character: 0 },
            }),
            range_length: None,
            text: "// English comment\n".to_string(),
        };

        doc.apply_changes(vec![change]).unwrap();
        assert!(doc.text().contains("English comment"));
        assert!(!doc.text().contains("中文"));
        doc.reparse().unwrap();
        assert_eq!(doc.tree.root_node().kind(), "source_file");
    }

    /// 测试 3：空 range 增量插入
    #[test]
    fn test_utf8_empty_range_insert() {
        let source = r#"// 注释
struct S { bit<8> x; }"#;
        let mut doc = new_doc(source);

        let change = TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position { line: 1, character: 7 },
                end: Position { line: 1, character: 7 },
            }),
            range_length: None,
            text: "New".to_string(),
        };

        doc.apply_changes(vec![change]).unwrap();
        assert!(doc.text().contains("struct NewS"));
    }

    /// 测试 4：全量替换
    #[test]
    fn test_full_replacement() {
        let source = r#"header H { bit<32> f; }"#;
        let mut doc = new_doc(source);

        let new_source = r#"struct S { bit<8> x; }"#;
        let change = TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: new_source.to_string(),
        };

        doc.apply_changes(vec![change]).unwrap();
        assert_eq!(doc.text(), new_source);
        doc.reparse().unwrap();
        let root = doc.tree.root_node();
        assert_eq!(root.kind(), "source_file");
        let child = root.child(0).unwrap();
        assert_eq!(child.kind(), "struct_definition");
    }

    /// 测试 5：Document::new Result 路径
    #[test]
    fn test_document_new_result_path() {
        let mut parser = crate::parser::new_parser().unwrap();
        let result = Document::new(
            Url::parse("file:///test.p4").unwrap(),
            r#"header H { bit<32> f; }"#.to_string(),
            &mut parser,
        );
        assert!(result.is_ok());
    }
}
