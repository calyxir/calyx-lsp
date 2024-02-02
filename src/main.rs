mod convert;
mod document;
mod goto_definition;
mod log;
mod ts_utils;

use std::collections::HashMap;
use std::fs;
use std::sync::RwLock;

use convert::Point;
use document::{ComponentSig, Document};
use goto_definition::{DefinitionProvider, QueryResult};
use serde::Deserialize;
use tower_lsp::lsp_types as lspt;
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
    /// A map from each open file, to the components defined in that file
    symbols: RwLock<HashMap<lspt::Url, HashMap<String, ComponentSig>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            open_docs: RwLock::new(HashMap::default()),
            config: RwLock::new(Config::default()),
            symbols: RwLock::new(HashMap::default()),
        }
    }

    fn open(&self, uri: lspt::Url, text: String) {
        let mut map = self.open_docs.write().unwrap();
        map.insert(uri.clone(), Document::new_with_text(uri, &text));
    }

    fn open_path(&self, uri: lspt::Url) {
        fs::read_to_string(uri.to_file_path().unwrap())
            .ok()
            .map(|text| self.open(uri.clone(), text));
    }

    fn exists(&self, uri: &lspt::Url) -> bool {
        let map = self.open_docs.read().unwrap();
        map.contains_key(uri)
    }

    fn read_document<F, T>(&self, uri: &lspt::Url, reader: F) -> Option<T>
    where
        F: FnMut(&Document) -> Option<T>,
    {
        let map = self.open_docs.read().unwrap();
        map.get(uri).and_then(reader)
    }

    fn read_and_open<F, T>(&self, uri: &lspt::Url, reader: F) -> Option<T>
    where
        F: FnMut(&Document) -> Option<T>,
    {
        // if the file doesnt exist, read it's contents and create a doc for it
        if !self.exists(&uri) {
            self.open_path(uri.clone());
            self.update_symbols(&uri);
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

    fn update_symbols(&self, url: &lspt::Url) {
        self.symbols
            .write()
            .unwrap()
            .entry(url.clone())
            .and_modify(|map| {
                self.read_document(url, |doc| {
                    for (name, sig) in doc.signatures() {
                        map.insert(name, sig);
                    }
                    Some(())
                });
            })
            .or_insert_with(|| {
                self.read_document(url, |doc| Some(doc.signatures().collect()))
                    .unwrap()
            });
        Debug::stdout(format!(
            "symbols: {:#?}",
            self.symbols
                .read()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.as_str(), v))
                .collect::<Vec<_>>()
        ));
    }
}

// /// Not yet sure where this should live. I'll just plop it here.
// fn resolve_imports<'a>(
//     cur_dir: PathBuf,
//     lib_paths: &'a [String],
//     imports: &'a [String],
// ) -> impl Iterator<Item = PathBuf> + 'a {
//     imports
//         .iter()
//         .cartesian_product(
//             vec![cur_dir]
//                 .into_iter()
//                 .chain(lib_paths.into_iter().map(|p| PathBuf::from(p))),
//         )
//         .map(|(base_path, lib_path)| lib_path.join(base_path).resolve().into_owned())
//         .filter(|p| p.exists())
// }

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
                completion_provider: Some(lspt::CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    all_commit_characters: None,
                    work_done_progress_options: Default::default(),
                    completion_item: None,
                }),
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
        self.update_symbols(&params.text_document.uri);
    }

    async fn goto_definition(
        &self,
        params: lspt::GotoDefinitionParams,
    ) -> jsonrpc::Result<Option<lspt::GotoDefinitionResponse>> {
        let url = &params.text_document_position_params.text_document.uri;
        let config = &self.config.read().unwrap();
        Ok(self
            .read_document(url, |doc| {
                doc.thing_at_point(params.text_document_position_params.position.into())
                    .and_then(|thing| doc.find_thing(config, url.clone(), thing))
            })
            .and_then(|gdr| match gdr {
                QueryResult::Found(loc) => Some(lspt::GotoDefinitionResponse::Scalar(loc)),
                QueryResult::ContinueSearch(paths, name) => {
                    let mut queue = paths;
                    let mut found = None;
                    while let Some(p) = queue.pop() {
                        let url = lspt::Url::from_file_path(p.clone()).unwrap();
                        let res = self.read_and_open(&url, |doc| {
                            doc.find_component(config, name.to_string())
                        });
                        match res {
                            Some(QueryResult::Found(loc)) => found = Some(loc),
                            Some(QueryResult::ContinueSearch(paths, _)) => {
                                queue.extend_from_slice(&paths)
                            }
                            None => (),
                        }
                    }
                    found.map(|loc| lspt::GotoDefinitionResponse::Scalar(loc))
                }
            }))
    }

    async fn completion(
        &self,
        params: lspt::CompletionParams,
    ) -> jsonrpc::Result<Option<lspt::CompletionResponse>> {
        let url = &params.text_document_position.text_document.uri;
        let point: Point = params.text_document_position.position.into();
        let config = self.config.read().unwrap();
        Ok(self
            .read_document(url, |doc| {
                Some(doc.completion_at_point(&config, point.clone()))
            })
            .and_then(|res| match res {
                QueryResult::Found(results) => Some(lspt::CompletionResponse::Array(
                    results
                        .into_iter()
                        .map(|(name, descr)| lspt::CompletionItem::new_simple(name, descr))
                        .collect(),
                )),
                QueryResult::ContinueSearch(paths, data) => {
                    Debug::stdout(format!("looking for {data} in {paths:?}"));
                    // open all paths recursively
                    let mut queue = paths;
                    let mut found = None;
                    while let Some(p) = queue.pop() {
                        Debug::stdout(format!("checking {p:?}"));
                        let url = lspt::Url::from_file_path(p.clone()).unwrap();
                        if !self.exists(&url) {
                            self.read_and_open(&url, |doc| {
                                queue.extend(doc.resolved_imports(&config));
                                Debug::stdout(format!("queue: {queue:?}"));
                                Some(())
                            });
                        }
                        self.update_symbols(&url);
                        if let Some(blah) = self
                            .symbols
                            .read()
                            .unwrap()
                            .get(&url)
                            .and_then(|map| map.get(&data))
                        {
                            found = Some(lspt::CompletionResponse::Array(
                                blah.inputs
                                    .iter()
                                    .map(|inp| (inp, "input"))
                                    .chain(blah.outputs.iter().map(|out| (out, "output")))
                                    .map(|(name, descr)| {
                                        lspt::CompletionItem::new_simple(
                                            name.to_string(),
                                            descr.to_string(),
                                        )
                                    })
                                    .collect(),
                            ));
                            Debug::stdout("breaking");
                            break;
                        }
                    }
                    found
                }
            }))
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        Debug::stdout("shutdown");
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
