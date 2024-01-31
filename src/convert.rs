//! Both lsp_types and tree_sitter use Point and Range types to represent
//! a position in a document, or a range in a document. This module contains
//! some definitions to make converting between them more ergonomic.

use tower_lsp::lsp_types as lspt;
use tree_sitter as ts;

/// Crate local Point representing a location in a document
#[derive(Clone)]
pub struct Point(ts::Point);

impl Point {
    pub fn zero() -> Self {
        Self(ts::Point { row: 0, column: 0 })
    }
}

impl Into<ts::Point> for Point {
    fn into(self) -> ts::Point {
        self.0
    }
}

impl From<ts::Point> for Point {
    fn from(value: ts::Point) -> Self {
        Point(value)
    }
}

impl Into<lspt::Position> for Point {
    fn into(self) -> lspt::Position {
        lspt::Position::new(self.0.row as u32, self.0.column as u32)
    }
}

impl From<lspt::Position> for Point {
    fn from(value: lspt::Position) -> Self {
        Point(ts::Point {
            row: value.line as usize,
            column: value.character as usize,
        })
    }
}

/// Crate local Range representing a region between two points
pub struct Range {
    start: Point,
    end: Point,
}

impl Range {
    pub fn zero() -> Self {
        Self {
            start: Point::zero(),
            end: Point::zero(),
        }
    }
}

impl<'a> From<ts::Node<'a>> for Range {
    fn from(value: ts::Node) -> Self {
        Range {
            start: value.start_position().into(),
            end: value.end_position().into(),
        }
    }
}

impl Into<lspt::Range> for Range {
    fn into(self) -> lspt::Range {
        lspt::Range::new(Point::from(self.start).into(), Point::from(self.end).into())
    }
}

impl From<lspt::Range> for Range {
    fn from(value: lspt::Range) -> Self {
        Range {
            start: value.start.into(),
            end: value.end.into(),
        }
    }
}

impl From<ts::Range> for Range {
    fn from(value: ts::Range) -> Self {
        Range {
            start: value.start_point.into(),
            end: value.end_point.into(),
        }
    }
}
