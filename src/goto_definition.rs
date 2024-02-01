use std::path::PathBuf;

use tower_lsp::lsp_types as lspt;
use tree_sitter as ts;

use crate::{
    convert::Range,
    document::{Document, Things},
    resolve_imports, Config,
};

#[derive(Debug)]
pub enum DefinitionResult<T> {
    Found(lspt::Location),
    ContinueSearch(Vec<PathBuf>, T),
}

pub trait DefinitionProvider<T> {
    fn find_thing(
        &self,
        config: &Config,
        url: lspt::Url,
        thing: Things,
    ) -> Option<DefinitionResult<T>> {
        match thing {
            Things::Cell(node, name) => self.find_cell(url, node, name),
            Things::SelfPort(node, name) => self.find_self_port(url, node, name),
            Things::Group(node, name) => self.find_group(url, node, name),
            Things::Import(_node, name) => self.find_import(config, url, name),
            Things::Component(name) => self.find_component(config, url, name),
        }
    }

    fn find_cell(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<DefinitionResult<T>>;
    fn find_self_port(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<DefinitionResult<T>>;
    fn find_group(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<DefinitionResult<T>>;
    fn find_import(
        &self,
        config: &Config,
        url: lspt::Url,
        name: String,
    ) -> Option<DefinitionResult<T>>;
    fn find_component(
        &self,
        config: &Config,
        url: lspt::Url,
        name: String,
    ) -> Option<DefinitionResult<T>>;
}

impl DefinitionProvider<String> for Document {
    fn find_cell(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<DefinitionResult<String>> {
        self.enclosing_cells(node)
            .find(|n| self.node_text(n) == name)
            .map(|node| DefinitionResult::Found(lspt::Location::new(url, Range::from(node).into())))
    }

    fn find_self_port(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<DefinitionResult<String>> {
        self.enclosing_component_ports(node)
            .find(|n| self.node_text(n) == name)
            .map(|n| {
                DefinitionResult::Found(lspt::Location::new(url.clone(), Range::from(n).into()))
            })
    }

    fn find_group(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<DefinitionResult<String>> {
        self.enclosing_groups(node)
            .find(|g| self.node_text(g) == name)
            .map(|node| {
                DefinitionResult::Found(lspt::Location::new(url.clone(), Range::from(node).into()))
            })
    }

    fn find_import(
        &self,
        config: &Config,
        url: lspt::Url,
        name: String,
    ) -> Option<DefinitionResult<String>> {
        resolve_imports(
            url.to_file_path().unwrap().parent().unwrap().to_path_buf(),
            &config.calyx_lsp.library_paths,
            &[name],
        )
        .next()
        .map(|path| {
            DefinitionResult::Found(lspt::Location::new(
                lspt::Url::parse(&format!("file://{}", path.display())).unwrap(),
                Range::zero().into(),
            ))
        })
    }

    fn find_component(
        &self,
        config: &Config,
        url: lspt::Url,
        name: String,
    ) -> Option<DefinitionResult<String>> {
        self.components()
            .find(|n| self.node_text(n) == name)
            .map(|n| {
                DefinitionResult::Found(lspt::Location::new(url.clone(), Range::from(n).into()))
            })
            .or_else(|| {
                Some(DefinitionResult::ContinueSearch(
                    resolve_imports(
                        url.to_file_path().unwrap().parent().unwrap().to_path_buf(),
                        &config.calyx_lsp.library_paths,
                        &self.imports(),
                    )
                    .collect(),
                    name,
                ))
            })
    }
}
