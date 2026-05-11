use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use std::collections::HashMap;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::{Node, Point, Tree};

use crate::semantic_tokens;
use crate::completion;
use crate::diagnostics::tree_diagnostics;
use crate::document::Document;
use crate::hover::{self, compute_active_parameter};
use crate::parser;
use crate::workspace::WorkspaceIndex;

pub struct Backend {
    client: Client,
    documents: DashMap<Url, Document>,
    workspace_index: WorkspaceIndex,
    workspace_folders: std::sync::Mutex<Vec<WorkspaceFolder>>,
    include_paths: std::sync::Mutex<Vec<String>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DashMap::new(),
            workspace_index: WorkspaceIndex::new(),
            workspace_folders: std::sync::Mutex::new(Vec::new()),
            include_paths: std::sync::Mutex::new(Vec::new()),
        }
    }

    async fn publish_diagnostics(&self, uri: &Url, tree: &Tree, source: &str) {
        let mut diagnostics = tree_diagnostics(tree, source, uri);
        let semantic = crate::diagnostics::semantic_diagnostics(tree, source, uri, &self.workspace_index);
        diagnostics.extend(semantic);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
    }

    /// 公共逻辑：给定位置和符号名，返回当前文件中所有引用位置。
    fn find_references_in_file(
        &self,
        uri: &Url,
        word: &str,
    ) -> Vec<Location> {
        let mut locations = Vec::new();
        if let Some(doc) = self.documents.get(uri) {
            let source = doc.text();
            let root = doc.tree.root_node();
            crate::references::collect_reference_nodes(root, &source, word, uri, &mut locations);
        }
        locations
    }

    fn resolve_definition(
        &self,
        uri: &Url,
        pos: Position,
    ) -> Option<GotoDefinitionResponse> {
        let doc = self.documents.get(uri)?;
        let source = doc.text();
        let word = word_at_pos(&source, pos)?;

        // 先查局部作用域
        let scope = self.workspace_index.scope_at(uri, pos, &doc.tree, &source);
        let in_locals = scope.params.iter().any(|(n, _)| n == &word)
            || scope.locals.iter().any(|(n, _)| n == &word);

        if in_locals {
            if let Some(range) =
                self.workspace_index.find_local_definition(uri, &doc.tree, &source, &word)
            {
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri: uri.clone(),
                    range,
                }));
            }
        }

        // 查全局索引
        let locations: Vec<Location> = self
            .workspace_index
            .resolve_symbol(&word, uri)
            .into_iter()
            .map(|(u, sym)| Location {
                uri: u,
                range: sym.to_lsp_range(),
            })
            .collect();

        if !locations.is_empty() {
            return Some(GotoDefinitionResponse::Array(locations));
        }

        None
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(folders) = params.workspace_folders {
            let mut wf = self.workspace_folders.lock().unwrap();
            *wf = folders;
        }

        // 读取 initializationOptions 中的 includePaths
        if let Some(opts) = params.initialization_options {
            if let Some(paths) = opts.get("includePaths").and_then(|v| v.as_array()) {
                let paths_clone = {
                    let mut include_paths = self.include_paths.lock().unwrap();
                    include_paths.clear();
                    for p in paths {
                        if let Some(s) = p.as_str() {
                            include_paths.push(s.to_string());
                        }
                    }
                    include_paths.clone()
                }; // MutexGuard 在此释放
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("p4lsp: includePaths = {:?}", paths_clone),
                    )
                    .await;
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        will_save: None,
                        will_save_wait_until: None,
                        save: None,
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(semantic_tokens::server_capabilities()),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                rename_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "p4lsp-server initialized")
            .await;

        let folders = {
            let wf = self.workspace_folders.lock().unwrap();
            wf.clone()
        };

        if folders.is_empty() {
            return;
        }

        let client = self.client.clone();
        let workspace_index = self.workspace_index.clone();

        tokio::spawn(async move {
            for folder in folders {
                let path = match folder.uri.to_file_path() {
                    Ok(p) => p,
                    Err(_) => {
                        client
                            .log_message(
                                MessageType::WARNING,
                                format!("p4lsp: cannot convert URI to path: {}", folder.uri),
                            )
                            .await;
                        continue;
                    }
                };

                let mut files_to_index = Vec::new();
                if let Err(e) = collect_p4_files(&path, &mut files_to_index).await {
                    client
                        .log_message(
                            MessageType::WARNING,
                            format!("p4lsp: failed to scan directory {}: {}", path.display(), e),
                        )
                        .await;
                    continue;
                }

                for file_path in files_to_index {
                    let uri = match Url::from_file_path(&file_path) {
                        Ok(u) => u,
                        Err(_) => {
                            client
                                .log_message(
                                    MessageType::WARNING,
                                    format!("p4lsp: cannot convert path to URI: {}", file_path.display()),
                                )
                                .await;
                            continue;
                        }
                    };

                    let text = match tokio::fs::read_to_string(&file_path).await {
                        Ok(t) => t,
                        Err(e) => {
                            client
                                .log_message(
                                    MessageType::WARNING,
                                    format!("p4lsp: failed to read file {}: {}", file_path.display(), e),
                                )
                                .await;
                            continue;
                        }
                    };

                    let mut parser = match parser::new_parser() {
                        Ok(p) => p,
                        Err(e) => {
                            client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("p4lsp: failed to create parser: {}", e),
                                )
                                .await;
                            continue;
                        }
                    };

                    let doc = match Document::new(uri.clone(), text.clone(), &mut parser) {
                        Ok(d) => d,
                        Err(e) => {
                            client
                                .log_message(
                                    MessageType::WARNING,
                                    format!("p4lsp: failed to parse {}: {}", uri, e),
                                )
                                .await;
                            continue;
                        }
                    };

                    workspace_index.index_document(&uri, &doc.tree, &doc.text());
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("p4lsp: indexed {}", uri),
                        )
                        .await;
                }
            }
        });
    }

    async fn shutdown(&self) -> Result<()> {
        self.documents.clear();
        // workspace_index 清理：遍历并移除所有文件索引
        for entry in self.workspace_index.files.iter() {
            self.workspace_index.remove_document(entry.key());
        }
        Ok(())
    }

    // --- Text Document Sync ---

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        let mut parser = match parser::new_parser() {
            Ok(p) => p,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("p4lsp: failed to create parser: {}", e))
                    .await;
                return;
            }
        };

        let doc = match Document::new(uri.clone(), text.clone(), &mut parser) {
            Ok(d) => d,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("p4lsp: failed to open {}: {}", uri, e))
                    .await;
                return;
            }
        };

        self.publish_diagnostics(&uri, &doc.tree, &text).await;
        self.workspace_index.index_document(&uri, &doc.tree, &doc.text());
        self.documents.insert(uri, doc);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(mut entry) = self.documents.get_mut(&uri) {
            if let Err(e) = entry.apply_changes(params.content_changes) {
                self.client
                    .log_message(MessageType::ERROR, format!("p4lsp: apply_changes failed: {}", e))
                    .await;
                let text = entry.text();
                match entry.parser.parse(&text, None) {
                    Some(tree) => {
                        entry.tree = tree;
                        entry.rope = Rope::from_str(&text);
                    }
                    None => {
                        self.client
                            .log_message(MessageType::ERROR, "p4lsp: fallback reparse also failed".to_string())
                            .await;
                        return;
                    }
                }
            }

            if let Err(e) = entry.reparse() {
                self.client
                    .log_message(MessageType::ERROR, format!("p4lsp: reparse failed: {}", e))
                    .await;
                return;
            }

            self.publish_diagnostics(&uri, &entry.tree, &entry.text()).await;
            self.workspace_index.index_document(&uri, &entry.tree, &entry.text());
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        self.workspace_index.remove_document(&uri);
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // --- Language Features ---

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        if let Some(doc) = self.documents.get(&uri) {
            let source = doc.text();

            // 先尝试局部作用域 hover
            let scope = self.workspace_index.scope_at(&uri, pos, &doc.tree, &source);
            let cursor_word = word_at_pos(&source, pos);
            if let Some(ref word) = cursor_word {
                for (name, ty) in &scope.params {
                    if name == word {
                        return Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!("**Parameter** `{}: {}`", name, ty),
                            }),
                            range: None,
                        }));
                    }
                }
                for (name, ty) in &scope.locals {
                    if name == word {
                        return Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!("**Local variable** `{}: {}`", name, ty),
                            }),
                            range: None,
                        }));
                    }
                }
            }

            let hover_result = hover::hover_with_workspace(&doc.tree, &source, pos, Some(&self.workspace_index), Some(&uri));
            return Ok(hover_result);
        }
        Ok(None)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        
        if let Some(doc) = self.documents.get(&uri) {
            let source = doc.text();
            // 查找光标所在位置的 call 节点
            let root = doc.tree.root_node();
            let point = Point {
                row: pos.line as usize,
                column: pos.character as usize,
            };
            
            if let Some(node) = root.descendant_for_point_range(point, point) {
                // 向上查找 call 上下文
                let mut current = node;
                let mut call_node = None;
                loop {
                    if current.kind() == "call" {
                        call_node = Some(current);
                        break;
                    }
                    if let Some(parent) = current.parent() {
                        current = parent;
                    } else {
                        break;
                    }
                }
                
                if let Some(call) = call_node {
                    // 提取 fval 和参数
                    let fval_opt = call.child(0).filter(|c| c.kind() == "fval");
                    let fval_text = fval_opt.map(|f| crate::hover::node_text(f, &source).to_string()).unwrap_or_default();
                    
                    // 查找方法签名
                    let sig = if let Some(sig_str) = crate::hover::find_method_signature_for_signature_help(&call, &fval_text, &source) {
                        sig_str
                    } else {
                        return Ok(None);
                    };
                    
                    // 计算当前激活的参数索引
                    let active_param = compute_active_parameter(&call, pos, &source);
                    
                    let signature = SignatureInformation {
                        label: sig,
                        documentation: None,
                        parameters: None,
                        active_parameter: Some(active_param),
                    };
                    
                    return Ok(Some(SignatureHelp {
                        signatures: vec![signature],
                        active_signature: Some(0),
                        active_parameter: Some(active_param),
                    }));
                }
            }
        }
        
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let trigger = params.context.as_ref().and_then(|c| c.trigger_character.as_deref());

        if let Some(doc) = self.documents.get(&uri) {
            let source = doc.text();
            let items = completion::completions(
                &uri,
                pos,
                &doc.tree,
                &source,
                &self.workspace_index,
                trigger,
            );
            return Ok(Some(CompletionResponse::Array(items)));
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(self.resolve_definition(&uri, pos))
    }

    async fn goto_declaration(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(self.resolve_definition(&uri, pos))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let word = if let Some(doc) = self.documents.get(&uri) {
            let source = doc.text();
            word_at_pos(&source, pos)
        } else {
            None
        };

        let word = match word {
            Some(w) => w,
            None => return Ok(None),
        };

        let mut locations = self.find_references_in_file(&uri, &word);

        // 跨文件搜索：遍历所有已索引的文件
        for entry in self.workspace_index.files.iter() {
            let file_uri = entry.key().clone();
            if file_uri == uri {
                continue;
            }
            let fi = entry.value();
            if let Some(ref file_tree) = fi.tree {
                crate::references::collect_reference_nodes(
                    file_tree.root_node(),
                    &fi.source,
                    &word,
                    &file_uri,
                    &mut locations,
                );
            }
        }

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        if let Some(doc) = self.documents.get(&params.text_document.uri) {
            let symbols = crate::index::document_symbols(doc.tree.root_node(), &doc.text());
            return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
        }
        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let new_name = params.new_name;

        if let Some(doc) = self.documents.get(&uri) {
            let source = doc.text();
            if let Some(targets) = crate::rename::prepare_rename(&uri, pos, &doc.tree, &source, &self.workspace_index) {
                let workspace_edit = crate::rename::build_workspace_edit(targets, new_name);
                return Ok(Some(workspace_edit));
            }
        }
        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            let tokens = semantic_tokens::semantic_tokens_full(&doc.tree, &doc.text());
            return Ok(Some(SemanticTokensResult::Tokens(tokens)));
        }
        Ok(None)
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            let tokens = semantic_tokens::semantic_tokens_range(&doc.tree, &doc.text(), params.range);
            return Ok(Some(SemanticTokensRangeResult::Tokens(tokens)));
        }
        Ok(None)
    }
}

fn word_at_pos(source: &str, pos: Position) -> Option<String> {
    let line = source.lines().nth(pos.line as usize)?;
    let target_cu = pos.character as usize;

    let mut cu = 0;
    let mut cursor_byte = None;
    for (byte_idx, c) in line.char_indices() {
        if cu >= target_cu {
            cursor_byte = Some(byte_idx);
            break;
        }
        cu += c.len_utf16();
    }
    let cursor_byte = cursor_byte?;

    let cursor_char = line[cursor_byte..].chars().next()?;
    if !is_word_char(cursor_char) {
        return None;
    }

    let mut start = cursor_byte;
    while start > 0 {
        let prev = line[..start].chars().next_back()?;
        if !is_word_char(prev) {
            break;
        }
        start -= prev.len_utf8();
    }

    let mut end = cursor_byte + cursor_char.len_utf8();
    while end < line.len() {
        let next = line[end..].chars().next()?;
        if !is_word_char(next) {
            break;
        }
        end += next.len_utf8();
    }

    Some(line[start..end].to_string())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Recursively collect all `.p4` files under a directory.
async fn collect_p4_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(collect_p4_files(&path, out)).await?;
        } else if path.extension().map_or(false, |ext| ext == "p4") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn parse_p4(source: &str) -> Tree {
        let mut p = parser::new_parser().expect("new_parser should succeed");
        p.parse(source, None).expect("parse should succeed")
    }

    #[tokio::test]
    async fn test_collect_p4_files_basic() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create files
        let mut f1 = std::fs::File::create(root.join("a.p4")).unwrap();
        f1.write_all(b"header h {}").unwrap();
        let mut f2 = std::fs::File::create(root.join("b.txt")).unwrap();
        f2.write_all(b"not p4").unwrap();

        let mut out = Vec::new();
        collect_p4_files(root, &mut out).await.unwrap();

        assert_eq!(out.len(), 1);
        assert!(out[0].ends_with("a.p4"));
    }

    #[tokio::test]
    async fn test_collect_p4_files_recursive() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::create_dir(root.join("sub")).unwrap();
        let mut f1 = std::fs::File::create(root.join("top.p4")).unwrap();
        f1.write_all(b"header h {}").unwrap();
        let mut f2 = std::fs::File::create(root.join("sub").join("nested.p4")).unwrap();
        f2.write_all(b"struct s {}").unwrap();

        let mut out = Vec::new();
        collect_p4_files(root, &mut out).await.unwrap();

        assert_eq!(out.len(), 2);
        let names: Vec<String> = out
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"top.p4".to_string()));
        assert!(names.contains(&"nested.p4".to_string()));
    }
}
