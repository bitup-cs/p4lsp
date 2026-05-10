use std::sync::OnceLock;
use tree_sitter::Language;

static LANGUAGE: OnceLock<Language> = OnceLock::new();

pub fn language() -> Language {
    LANGUAGE.get_or_init(|| {
        extern "C" {
            fn tree_sitter_p4() -> Language;
        }
        unsafe { tree_sitter_p4() }
    }).clone()
}

/// 创建一个已配置好 P4 语言的 Parser。
pub fn new_parser() -> Result<tree_sitter::Parser, String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language())
        .map_err(|e| format!("failed to set P4 language: {:?}", e))?;
    Ok(parser)
}
