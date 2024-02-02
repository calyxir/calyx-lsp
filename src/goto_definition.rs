use std::path::PathBuf;

use tower_lsp::lsp_types as lspt;
use tree_sitter as ts;

use crate::{
    convert::Range,
    document::{Document, Things},
    Config,
};

#[derive(Debug)]
pub enum QueryResult<F, C> {
    Found(F),
    ContinueSearch(Vec<PathBuf>, C),
}

pub trait DefinitionProvider {
    fn find_thing(
        &self,
        config: &Config,
        url: lspt::Url,
        thing: Things,
    ) -> Option<QueryResult<lspt::Location, String>> {
        match thing {
            Things::Cell(node, name) => self.find_cell(url, node, name),
            Things::SelfPort(node, name) => self.find_self_port(url, node, name),
            Things::Group(node, name) => self.find_group(url, node, name),
            Things::Import(_node, name) => self.find_import(config, url, name),
            Things::Component(name) => self.find_component(config, name),
        }
    }

    fn find_cell(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>>;
    fn find_self_port(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>>;
    fn find_group(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>>;
    fn find_import(
        &self,
        config: &Config,
        url: lspt::Url,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>>;
    fn find_component(
        &self,
        config: &Config,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>>;
}

impl DefinitionProvider for Document {
    fn find_cell(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>> {
        self.enclosing_cells(node)
            .find(|n| self.node_text(n) == name)
            .map(|node| QueryResult::Found(lspt::Location::new(url, Range::from(node).into())))
    }

    fn find_self_port(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>> {
        self.enclosing_component_ports(node)
            .find(|n| self.node_text(n) == name)
            .map(|n| QueryResult::Found(lspt::Location::new(url.clone(), Range::from(n).into())))
    }

    fn find_group(
        &self,
        url: lspt::Url,
        node: ts::Node,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>> {
        self.enclosing_groups(node)
            .find(|g| self.node_text(g) == name)
            .map(|node| {
                QueryResult::Found(lspt::Location::new(url.clone(), Range::from(node).into()))
            })
    }

    fn find_import(
        &self,
        _config: &Config,
        _url: lspt::Url,
        _name: String,
    ) -> Option<QueryResult<lspt::Location, String>> {
        None
        // self.resolved_imports(config)
        // resolve_imports(
        //     url.to_file_path().unwrap().parent().unwrap().to_path_buf(),
        //     &config.calyx_lsp.library_paths,
        //     &[name],
        // )
        // .next()
        // .map(|path| {
        //     QueryResult::Found(lspt::Location::new(
        //         lspt::Url::parse(&format!("file://{}", path.display())).unwrap(),
        //         Range::zero().into(),
        //     ))
        // })
    }

    fn find_component(
        &self,
        config: &Config,
        name: String,
    ) -> Option<QueryResult<lspt::Location, String>> {
        self.components()
            .find(|n| self.node_text(n) == name)
            .map(|n| {
                QueryResult::Found(lspt::Location::new(self.url.clone(), Range::from(n).into()))
            })
            .or_else(|| {
                Some(QueryResult::ContinueSearch(
                    self.resolved_imports(config).collect(),
                    name,
                ))
            })
    }
}
