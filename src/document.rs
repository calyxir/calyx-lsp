//! Represents a single Calyx file

use std::collections::HashMap;

use itertools::Itertools;
use tree_sitter::{Node, Parser, Point, Query, QueryCursor, Tree};

use crate::log::Debug;
use crate::tree_sitter_calyx;
use crate::ts_utils::ParentUntil;

pub struct Document {
    text: String,
    tree: Option<Tree>,
}

pub enum Things<'a> {
    /// Identifier referring to a cell
    Cell(Node<'a>, String),
    /// Identifier referring to a port
    Port(Node<'a>, String),
    /// Identifier refeferring to a component
    #[allow(unused)]
    Component(Node<'a>, String),
}

impl Document {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            tree: None,
        }
    }

    pub fn new_with_text(parser: &mut Parser, text: &str) -> Self {
        let mut doc = Self::new();
        doc.parse_whole_text(parser, text);
        doc
    }

    pub fn parse_whole_text(&mut self, parser: &mut Parser, text: &str) {
        self.text = text.to_string();
        self.tree = parser.parse(text, None);
        Debug::update("tree", self.tree.as_ref().unwrap().root_node().to_sexp())
    }

    fn captures<'a, 'node: 'a>(
        &'a self,
        pattern: &str,
        node: Node<'node>,
    ) -> HashMap<String, Vec<Node>> {
        let mut cursor = QueryCursor::new();
        let query = Query::new(unsafe { tree_sitter_calyx() }, pattern).expect("Invalid query");
        let capture_names = query.capture_names();
        let mut map = HashMap::default();
        for (capture, idx) in cursor.captures(&query, node, self.text.as_bytes()) {
            Debug::log("stdout", format!("{idx} -> {capture:?}"));
            let captured_nodes = capture.captures.iter().map(|c| c.node).collect_vec();
            map.entry(capture_names[idx].to_string())
                .and_modify(|e: &mut Vec<Node>| {
                    e.extend(&captured_nodes);
                })
                .or_insert(captured_nodes);
        }
        map
    }

    pub fn cells_from<'a>(&'a self, node: Node<'a>, name: &str) -> Option<Node<'a>> {
        node.parent_until(|n| n.kind() == "component")
            .and_then(|comp| {
                let comp_node = self.captures("(component) @comp", comp)["comp"][0];
                let cells = &self.captures("(cell_assignment (ident) @cell)", comp_node)["cell"];
                Debug::log(
                    "stdout",
                    format!(
                        "{}",
                        cells
                            .iter()
                            .map(|x| format!("{x:?}"))
                            .collect_vec()
                            .join(", ")
                    ),
                );
                cells.iter().find(|n| self.node_text(n) == name).cloned()
            })
    }

    pub fn node_at_point(&self, point: Point) -> Option<Node> {
        self.tree
            .as_ref()
            .and_then(|t| t.root_node().descendant_for_point_range(point, point))
    }

    pub fn thing_at_point(&self, point: Point) -> Option<Things> {
        self.node_at_point(point).and_then(|node| {
            if node.parent().is_some_and(|p| p.kind() == "port") {
                if node.next_sibling().is_some() {
                    Some(Things::Cell(
                        node.clone(),
                        self.node_text(&node).to_string(),
                    ))
                } else {
                    Some(Things::Port(
                        node.clone(),
                        self.node_text(&node).to_string(),
                    ))
                }
            } else {
                None
            }
        })
    }

    pub fn node_text(&self, node: &Node) -> &str {
        node.utf8_text(self.text.as_bytes()).unwrap()
    }
}

// Maybe useful functions for some point later
// -------
// fn apply_line_bytes_edit(&self, event: &lspt::TextDocumentContentChangeEvent) {
//     let mut lbs = self.line_bytes.write().unwrap();
//     if let Some(range) = event.range {
//         // take all the lines in the range, and replace them with the lines in event.text
//         // the number of newlines more than the line span is the number of new lines we need
//         // to include

//         let mut new_region = newline_split(&event.text)
//             .iter()
//             .map(|line| line.len())
//             .collect::<Vec<_>>();

//         if (range.start.line as usize) < lbs.len() {
//             // TODO: use a more efficient data structure than a Vec
//             // first we split off the vector at the beginning of the range
//             let mut specified_region = lbs.split_off(range.start.line as usize);
//             let second_half =
//                 specified_region.split_off((range.end.line - range.start.line) as usize);

//             // we have to correct the new region.
//             // example:
//             //          ↓ n_bytes_before
//             // xxxxxxxxxx-----------
//             // -----------
//             // -----------xxx
//             //            ↑ n_bytes_after
//             let n_bytes_before = range.start.character as usize;
//             let n_bytes_after = second_half[0] - range.end.character as usize;

//             // correct the line counts for the start and end of the new region
//             new_region.first_mut().map(|el| *el += n_bytes_before);
//             new_region.last_mut().map(|el| *el += n_bytes_after);

//             // then we insert the new region inbetween
//             lbs.append(&mut new_region);
//             lbs.extend_from_slice(&second_half[1..]);
//         } else {
//             lbs.append(&mut new_region);
//         }
//     } else {
//         todo!("Not sure what it means if we have no range.")
//     }
// }

// fn update_parse_tree(&self, event: &lspt::TextDocumentContentChangeEvent) {
//     let mut parser = self.parser.write().unwrap();
//     let mut tree = self.tree.write().unwrap();

//     if let Some(range) = event.range {
//         let lines = event.text.split('\n').collect::<Vec<_>>();
//         let start_position = range.start.point();
//         let old_end_position = range.end.point();
//         let new_end_position = if lines.len() == 1 {
//             Point::new(
//                 range.start.line as usize,
//                 (range.start.character as usize) + event.text.len(),
//             )
//         } else {
//             Point::new(
//                 (range.start.line as usize) + (lines.len() - 1),
//                 lines.last().unwrap().len(),
//             )
//         };
//         let start_byte = self.point_to_byte_offset(&start_position);
//         let old_end_byte = self.point_to_byte_offset(&old_end_position);
//         let new_end_byte = start_byte + event.text.len();

//         let input_edit = InputEdit {
//             start_byte,
//             old_end_byte,
//             new_end_byte,
//             start_position,
//             old_end_position,
//             new_end_position,
//         };
//         // debug
//         self.debug_log("stdout", &format!("{input_edit:#?}"));
//         let d = tree
//             .as_ref()
//             .unwrap()
//             .root_node()
//             .descendant_for_byte_range(start_byte, old_end_byte)
//             .unwrap()
//             .to_sexp();
//         self.debug_log("stdout", &format!("{d}"));

//         let new_tree = tree.as_mut().and_then(|t| {
//             t.edit(&input_edit);
//             parser.parse(&event.text, Some(t))
//         });
//         *tree = new_tree;
//     }
// }

// fn point_to_byte_offset(&self, point: &Point) -> usize {
//     let lbs = self.line_bytes.read().unwrap();
//     lbs[0..point.row].iter().sum::<usize>() + point.column
// }
