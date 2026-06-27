use std::str::FromStr;


use shared::id;
use strum_macros::{AsRefStr, FromRepr};

use crate::prelude::{NodeId};

pub mod prelude;
pub mod editor;
pub mod iter;
pub mod crdt;

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct ContentId(shared::id::ID);

impl ContentId {
    pub fn new() -> Self {
        ContentId(id::generate_id())
    }
}

impl NodeId for ContentId {}

impl FromStr for ContentId {
    type Err = <shared::id::ID as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

pub struct Node {
    id: String,
    parent: Option<ContentId>,
    next_sibling: Option<ContentId>,
    prev_sibling: Option<ContentId>,
    spec: NodeSpec
}


#[derive(Hash, Debug, Clone, strum_macros::EnumDiscriminants)]
#[strum_discriminants(derive(strum_macros::AsRefStr), name(NodeKind))]pub enum NodeSpec {
    Root,
    Paragraph(Paragraph),
    Plain(String),
    Span(Span),
    /// List
    List(List),
    ListItem(ListItem),
    Table(Table),
    Row(Row),
    Cell(Cell)
}


impl From<Paragraph> for NodeSpec {
    fn from(value: Paragraph) -> Self {
        NodeSpec::Paragraph(value)
    }
}

impl From<List> for NodeSpec {
    fn from(value: List) -> Self {
        NodeSpec::List(value)
    }
}

impl From<ListItem> for NodeSpec {
    fn from(value: ListItem) -> Self {
        NodeSpec::ListItem(value)
    }
}

impl From<Span> for NodeSpec {
    fn from(value: Span) -> Self {
        NodeSpec::Span(value)
    }
}

impl From<Table> for NodeSpec {
    fn from(value: Table) -> Self {
        NodeSpec::Table(value)
    }
}

impl From<Row> for NodeSpec {
    fn from(value: Row) -> Self {
        NodeSpec::Row(value)
    }
}

impl From<Cell> for NodeSpec {
    fn from(value: Cell) -> Self {
        NodeSpec::Cell(value)
    }
}

#[derive(Hash, Debug, Clone, Default)]
pub struct Span {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool
}

#[derive(Hash, Debug, Clone, Default)]
pub struct ListItem;

#[derive(Hash, Debug, Clone, Default)]
pub struct List {
    marker: ListMarker,
    start: Option<u32>,
}

#[derive(Hash, Debug, Clone, Default)]
pub enum ListMarker {
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,      // 1, 2, 3
    LowerAlpha,   // a, b, c
    UpperAlpha,   // A, B, C
    LowerRoman,   // i, ii, iii
    UpperRoman,   // I, II, III
}

#[derive(Hash, Debug, Clone, Default)]
pub struct ListKind;

#[derive(Hash, Debug, Clone, Default)]
pub struct Paragraph;

#[derive(Hash, Debug, Clone, Default)]
pub struct Table;

#[derive(Hash, Debug, Clone, Default)]
pub struct Row;

#[derive(Hash, Debug, Clone, Default)]
pub struct Cell;
