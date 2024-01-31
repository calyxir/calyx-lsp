mod convert;
mod document;
mod log;
mod ts_utils;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use convert::Range;
use document::Document;
use itertools::Itertools;
use resolve_path::PathResolveExt;
use serde::Deserialize;
use tower_lsp::lsp_types::{self as lspt, GotoDefinitionParams, GotoDefinitionResponse, Location};
use tower_lsp::{jsonrpc, Client, LanguageServer, LspService, Server};
use tree_sitter as ts;

use crate::log::Debug;

extern "C" {
    fn tree_sitter_calyx() -> ts::Language;
}

#[derive(Debug, Deserialize, Default)]
struct Config {
    #[serde(rename = "calyx-lsp")]
    calyx_lsp: CalyxLspConfig,
}

#[derive(Debug, Deserialize)]
struct CalyxLspConfig {
    #[serde(rename = "library-paths")]
    library_paths: Vec<String>,
}

impl Default for CalyxLspConfig {
    fn default() -> Self {
        Self {
            library_paths: vec!["~/.calyx".to_string()],
        }
    }
}

struct Backend {
    client: Client,
    open_docs: RwLock<HashMap<lspt::Url, document::Document>>,
    config: RwLock<Config>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            open_docs: RwLock::new(HashMap::default()),
            config: RwLock::new(Config::default()),
        }
    }

    fn open(&self, uri: lspt::Url, text: String) {
        let mut map = self.open_docs.write().unwrap();
        map.insert(uri, Document::new_with_text(&text));
    }

    fn exists(&self, uri: &lspt::Url) -> bool {
        let map = self.open_docs.read().unwrap();
        map.contains_key(uri)
    }

    fn read_document<F, T>(&self, uri: &lspt::Url, reader: F) -> Option<T>
    where
        F: Fn(&Document) -> Option<T>,
    {
        let map = self.open_docs.read().unwrap();
        map.get(uri).and_then(reader)
    }

    fn read_and_open<F, T>(&self, uri: lspt::Url, reader: F) -> Option<T>
    where
        F: Fn(&Document) -> Option<T>,
    {
        // if the file doesnt exist, read it's contents and create a doc for it
        if !self.exists(&uri) {
            fs::read_to_string(uri.to_file_path().unwrap())
                .ok()
                .map(|text| {
                    self.open(uri.clone(), text);
                });
        }

        self.read_document(&uri, reader)
    }

    fn update<F>(&self, uri: &lspt::Url, updater: F)
    where
        F: FnMut(&mut Document) -> (),
    {
        let mut map = self.open_docs.write().unwrap();
        map.get_mut(uri).map(updater);
    }

    /// Not yet sure where this should live. I'll just plop it here.
    fn resolve_imports<'a>(
        &'a self,
        cur_dir: PathBuf,
        imports: &'a [String],
    ) -> impl Iterator<Item = PathBuf> + 'a {
        Debug::stdout(format!("{cur_dir:?} {imports:?}"));
        let config = self.config.read().unwrap();
        let lib_paths = config.calyx_lsp.library_paths.clone();
        imports
            .iter()
            .cartesian_product(
                vec![cur_dir]
                    .into_iter()
                    .chain(lib_paths.into_iter().map(|p| PathBuf::from(p))),
            )
            .map(|(base_path, lib_path)| lib_path.join(base_path).resolve().into_owned())
            .filter(|p| p.exists())
    }
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

#[derive(Debug)]
enum GotoDefResult<T> {
    Found(lspt::Location),
    ContinueSearch(Vec<PathBuf>, T),
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(
        &self,
        _ip: lspt::InitializeParams,
    ) -> jsonrpc::Result<lspt::InitializeResult> {
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
                hover_provider: None,
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
        self.open(params.text_document.uri.clone(), params.text_document.text);
    }

    async fn did_change_configuration(&self, params: lspt::DidChangeConfigurationParams) {
        Debug::stdout(format!("{}", params.settings));
        let config: Config = serde_json::from_value(params.settings).unwrap();
        *self.config.write().unwrap() = config;
    }

    async fn did_change(&self, params: lspt::DidChangeTextDocumentParams) {
        self.update(&params.text_document.uri, |doc| {
            for event in &params.content_changes {
                doc.parse_whole_text(&event.text);
            }
        });
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> jsonrpc::Result<Option<lspt::GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let res: Option<GotoDefResult<String>> = self.read_document(uri, |doc| {
            doc.thing_at_point(params.text_document_position_params.position.into())
                .and_then(|thing| match thing {
                    document::Things::Cell(node, name) => doc
                        .enclosing_cells(node)
                        .find(|n| doc.node_text(n) == name)
                        .map(|node| {
                            GotoDefResult::Found(Location::new(
                                uri.clone(),
                                Range::from(node).into(),
                            ))
                        }),
                    document::Things::SelfPort(node, name) => doc
                        .enclosing_component_ports(node)
                        .find(|n| doc.node_text(n) == name)
                        .map(|n| {
                            GotoDefResult::Found(Location::new(uri.clone(), Range::from(n).into()))
                        }),
                    document::Things::Component(name) => doc
                        .components()
                        .find(|n| doc.node_text(n) == name)
                        .map(|n| {
                            GotoDefResult::Found(Location::new(uri.clone(), Range::from(n).into()))
                        })
                        .or_else(|| {
                            Some(GotoDefResult::ContinueSearch(
                                self.resolve_imports(
                                    uri.to_file_path().unwrap().parent().unwrap().to_path_buf(),
                                    &doc.imports(),
                                )
                                .collect(),
                                name,
                            ))
                        }),
                    document::Things::Group(node, name) => doc
                        .enclosing_groups(node)
                        .find(|g| doc.node_text(g) == name)
                        .map(|node| {
                            GotoDefResult::Found(Location::new(
                                uri.clone(),
                                Range::from(node).into(),
                            ))
                        }),
                    document::Things::Import(_node, name) => {
                        let paths = self
                            .resolve_imports(
                                uri.to_file_path().unwrap().parent().unwrap().to_path_buf(),
                                &[name],
                            )
                            .collect_vec();
                        if paths.len() > 0 {
                            Some(GotoDefResult::Found(Location::new(
                                lspt::Url::parse(&format!("file://{}", paths[0].display()))
                                    .unwrap(),
                                Range::zero().into(),
                            )))
                        } else {
                            None
                        }
                    }
                })
        });
        Debug::stdout(format!("goto/def: {res:?}"));
        Ok(res.and_then(|gdr| match gdr {
            GotoDefResult::Found(loc) => Some(GotoDefinitionResponse::Scalar(loc)),
            GotoDefResult::ContinueSearch(paths, name) => {
                let mut queue = paths;
                let mut found = None;
                while let Some(p) = queue.pop() {
                    let res =
                        self.read_and_open(lspt::Url::from_file_path(p.clone()).unwrap(), |doc| {
                            doc.components()
                                .find(|n| doc.node_text(n) == name)
                                .map(|n| {
                                    GotoDefResult::Found(Location::new(
                                        lspt::Url::from_file_path(p.clone()).unwrap(),
                                        Range::from(n).into(),
                                    ))
                                })
                                .or_else(|| {
                                    Some(GotoDefResult::ContinueSearch(
                                        self.resolve_imports(
                                            p.parent().unwrap().to_path_buf(),
                                            &doc.imports(),
                                        )
                                        .collect(),
                                        name.to_string(),
                                    ))
                                })
                        });
                    match res.unwrap() {
                        GotoDefResult::Found(loc) => found = Some(loc),
                        GotoDefResult::ContinueSearch(paths, _) => queue.extend_from_slice(&paths),
                    }
                }
                found.map(|loc| {
                    Debug::stdout(format!("found {loc:?}"));
                    GotoDefinitionResponse::Scalar(loc)
                })
            }
        }))
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
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
