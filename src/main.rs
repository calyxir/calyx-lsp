mod document;
mod log;
mod ts_utils;

use std::collections::HashMap;
use std::sync::RwLock;

use document::Document;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{self as lspt, GotoDefinitionParams};
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tree_sitter::{Language, Parser, Point};

use crate::log::Debug;

extern "C" {
    fn tree_sitter_calyx() -> Language;
}

struct Backend {
    client: Client,
    parser: RwLock<Parser>,
    open_docs: RwLock<HashMap<lspt::Url, document::Document>>,
}

trait ToPoint {
    fn point(&self) -> Point;
}

trait ToPosition {
    fn position(&self) -> lspt::Position;
}

impl ToPoint for lspt::Position {
    fn point(&self) -> Point {
        Point::new(self.line as usize, self.character as usize)
    }
}

impl ToPosition for Point {
    fn position(&self) -> lspt::Position {
        lspt::Position {
            line: self.row as u32,
            character: self.column as u32,
        }
    }
}

impl Backend {
    fn new(client: Client) -> Self {
        // create the tree-sitter parser
        let language = unsafe { tree_sitter_calyx() };
        let mut parser = Parser::new();
        parser.set_language(language).unwrap();

        Self {
            client,
            parser: RwLock::new(parser),
            open_docs: RwLock::new(HashMap::default()),
        }
    }

    fn open(&self, uri: lspt::Url, text: String) {
        let mut map = self.open_docs.write().unwrap();
        map.insert(
            uri,
            Document::new_with_text(&mut self.parser.write().unwrap(), &text),
        );
    }

    fn read_document<F, T>(&self, uri: &lspt::Url, reader: F) -> Option<T>
    where
        F: Fn(&Document) -> Option<T>,
    {
        let map = self.open_docs.read().unwrap();
        map.get(uri).and_then(reader)
    }

    fn update<F>(&self, uri: &lspt::Url, updater: F)
    where
        F: FnMut(&mut Document) -> (),
    {
        let mut map = self.open_docs.write().unwrap();
        map.get_mut(uri).map(updater);
    }

    // fn parse_whole_document(&self, text: &str) {
    //     let mut parser = self.parser.write().unwrap();
    //     let mut tree = self.tree.write().unwrap();
    //     *tree = parser.parse(text, None);
    //     let s = tree.as_ref().map(|t| t.root_node().to_sexp());
    //     self.debug("tree", format!("{}", s.unwrap_or("nil".to_string())));

    //     *self.open_docs.write().unwrap() = text.to_string();
    // }
}

/// TODO: turn this into a trait
fn newline_split(data: &str) -> Vec<String> {
    let mut res = vec![];
    let mut curr_string = String::new();
    for c in data.chars() {
        if c == '\n' {
            res.push(curr_string);
            curr_string = String::new();
        } else {
            curr_string.push(c);
        }
    }
    res.push(curr_string);
    res
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _ip: lspt::InitializeParams) -> Result<lspt::InitializeResult> {
        Debug::log("stdout", "init");
        assert_eq!(newline_split("\n").len(), 2);
        Ok(lspt::InitializeResult {
            server_info: None,
            capabilities: lspt::ServerCapabilities {
                // TODO: switch to incremental parsing
                text_document_sync: Some(lspt::TextDocumentSyncCapability::Kind(
                    lspt::TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(lspt::OneOf::Left(true)),
                hover_provider: Some(lspt::HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _ip: lspt::InitializedParams) {
        self.client
            .log_message(lspt::MessageType::INFO, "server initialized!")
            .await;
    }

    async fn did_open(&self, params: lspt::DidOpenTextDocumentParams) {
        self.open(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: lspt::DidChangeTextDocumentParams) {
        // apply all the text_edits
        let mut parser = self.parser.write().unwrap();
        self.update(&params.text_document.uri, |doc| {
            for event in &params.content_changes {
                doc.parse_whole_text(&mut parser, &event.text);
            }
        });
    }

    async fn hover(&self, hover_params: lspt::HoverParams) -> Result<Option<lspt::Hover>> {
        let params = hover_params.text_document_position_params;
        Ok(self.read_document(&params.text_document.uri, |doc| {
            doc.thing_at_point(params.position.point())
                .and_then(|thing| {
                    match thing {
                        document::Things::Cell(node, name) => Some(lspt::Hover {
                            contents: lspt::HoverContents::Scalar(lspt::MarkedString::String(
                                format!("cell: {name}"),
                            )),
                            range: Some(lspt::Range {
                                start: node.start_position().position(),
                                end: node.end_position().position(),
                            }),
                        }),
                        document::Things::Port(node, name) => Some(lspt::Hover {
                            contents: lspt::HoverContents::Scalar(lspt::MarkedString::String(
                                format!("port: {name}"),
                            )),
                            range: Some(lspt::Range {
                                start: node.start_position().position(),
                                end: node.end_position().position(),
                            }),
                        }),
                        document::Things::Component(..) => None,
                    }
                    // if node.kind() == "ident" {
                    //     let name = doc.node_text(&node);

                    // } else {
                    //     None
                    // }
                })
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<lspt::GotoDefinitionResponse>> {
        Debug::log("stdout", format!("{params:#?}"));
        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

// #[tokio::main]
// async fn main() {
//     tracing_subscriber::fmt().with_ansi(false).init();

//     let listener = TcpListener::bind("127.0.0.1:9257").await.unwrap();
//     println!("waiting for somebody to connect");
//     let (stream, _) = listener.accept().await.unwrap();
//     println!("connected! {stream:?}");

//     let (read, write) = tokio::io::split(stream);

//     // create the tree-sitter parser
//     let language = unsafe { tree_sitter_calyx() };
//     let mut parser = Parser::new();
//     parser.set_language(language).unwrap();

//     let (service, socket) = LspService::new(|client| Backend::new(client));
//     println!("starting");
//     Server::new(read, write, socket).serve(service).await;
//     println!("done");
// }
