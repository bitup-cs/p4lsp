use tower_lsp::{LspService, Server};

use p4lsp_server::parser;
use p4lsp_server::server::Backend;

#[tokio::main]
async fn main() {
    env_logger::init();

    // 预热 parser（加载 tree-sitter 语言）
    let _ = parser::language();

    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
