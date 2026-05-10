use std::cell::RefCell;
use tree_sitter::Parser;

thread_local! {
    static PARSER: RefCell<Option<Parser>> = RefCell::new(None);
}

/// 获取一个已初始化的 thread-local Parser（惰性创建）。
/// 如果 grammar 加载失败，返回 Err。
pub fn with_parser<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut Parser) -> R,
{
    PARSER.with(|p| {
        let mut opt = p.borrow_mut();
        if opt.is_none() {
            let mut parser = Parser::new();
            parser
                .set_language(&super::language())
                .map_err(|e| format!("failed to set P4 language: {:?}", e))?;
            *opt = Some(parser);
        }
        Ok(f(opt.as_mut().unwrap()))
    })
}
