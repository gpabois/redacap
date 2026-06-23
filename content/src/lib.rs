use std::str::FromStr;


use crate::prelude::{ContentNodeIdModel};

pub mod prelude;
pub mod editor;
pub mod iter;
pub mod crdt;

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct ContentId(shared::id::ID);

impl ContentNodeIdModel for ContentId {}

impl FromStr for ContentId {
    type Err = <shared::id::ID as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

pub struct ContentNode {
    id: String,
    parent: Option<ContentId>,
    next_sibling: Option<ContentId>,
    prev_sibling: Option<ContentId>,
    spec: ContentNodeSpec
}


#[derive(Hash, Debug, Clone, strum_macros::EnumDiscriminants)]
#[strum_discriminants(derive(strum_macros::AsRefStr), name(ContentNodeKind))]pub enum ContentNodeSpec {
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


impl From<Paragraph> for ContentNodeSpec {
    fn from(value: Paragraph) -> Self {
        ContentNodeSpec::Paragraph(value)
    }
}

impl From<List> for ContentNodeSpec {
    fn from(value: List) -> Self {
        ContentNodeSpec::List(value)
    }
}

impl From<ListItem> for ContentNodeSpec {
    fn from(value: ListItem) -> Self {
        ContentNodeSpec::ListItem(value)
    }
}

impl From<Span> for ContentNodeSpec {
    fn from(value: Span) -> Self {
        ContentNodeSpec::Span(value)
    }
}

impl From<Table> for ContentNodeSpec {
    fn from(value: Table) -> Self {
        ContentNodeSpec::Table(value)
    }
}

impl From<Row> for ContentNodeSpec {
    fn from(value: Row) -> Self {
        ContentNodeSpec::Row(value)
    }
}

impl From<Cell> for ContentNodeSpec {
    fn from(value: Cell) -> Self {
        ContentNodeSpec::Cell(value)
    }
}

#[derive(Hash, Debug, Clone, Default)]
pub struct Span;

#[derive(Hash, Debug, Clone, Default)]
pub struct ListItem;

#[derive(Hash, Debug, Clone, Default)]
pub struct List;

#[derive(Hash, Debug, Clone, Default)]
pub struct Paragraph;

#[derive(Hash, Debug, Clone, Default)]
pub struct Table;

#[derive(Hash, Debug, Clone, Default)]
pub struct Row;

#[derive(Hash, Debug, Clone, Default)]
pub struct Cell;
