use shared::id;
use strum_macros::{EnumString, IntoStaticStr};

use crate::{prelude::{NodeId, ReadableNodeSpecDef}, traits::WritableNodeSpecDef};

pub mod prelude;
pub mod editor;
pub mod iter;
pub mod crdt;
pub mod traits;

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct ContentId(shared::id::ID);

impl<'a> TryFrom<&'a [u8]> for ContentId {
    type Error = <shared::id::ID as TryFrom<&'a [u8]>>::Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        value.try_into().map(Self)
    }
}

impl ContentId {
    pub fn new() -> Self {
        ContentId(id::generate_id())
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl NodeId for ContentId {}

#[derive(strum_macros::EnumString, strum_macros::IntoStaticStr)]
pub enum NodeKind {
    Root,
    Paragraph,
    Plain,
    Span,
    List,
    ListItem,
    Table,
    Row,
    Cell
}

pub enum RefNodeSpec<'a, C: ReadableNodeSpecDef<'a>> {
    Root(C::RefRoot),
    Paragraph(C::RefParagraph),
    Plain(C::RefPlain),
    Span(C::RefSpan),
    List(C::RefList),
    ListItem(C::RefListItem),
    Table(C::RefTable),
    Row(C::RefRow),
    Cell(C::RefCell)
}

pub enum MutNodeSpec<'a, C: WritableNodeSpecDef<'a>> {
    Root(C::MutRoot),
    Paragraph(C::MutParagraph),
    Plain(C::MutPlain),
    Span(C::MutSpan),
    List(C::MutList),
    ListItem(C::MutListItem),
    Table(C::MutTable),
    Row(C::MutRow),
    Cell(C::MutCell)
}

pub enum NodeSpec {
    Root(Root),
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

impl From<&NodeSpec> for NodeKind {
    fn from(value: &NodeSpec) -> Self {
        value.kind()
    }
}

impl NodeSpec {
    pub fn kind(&self) -> NodeKind {
        match self {
            NodeSpec::Root(root) => NodeKind::Root,
            NodeSpec::Paragraph(paragraph) => NodeKind::Paragraph,
            NodeSpec::Plain(_) => NodeKind::Plain,
            NodeSpec::Span(span) => NodeKind::Span,
            NodeSpec::List(list) => NodeKind::List,
            NodeSpec::ListItem(list_item) => NodeKind::ListItem,
            NodeSpec::Table(table) => NodeKind::Table,
            NodeSpec::Row(row) => NodeKind::Row,
            NodeSpec::Cell(cell) => NodeKind::Cell,
        }
    }
}

impl From<Root> for NodeSpec {
    fn from(value: Root) -> Self {
        NodeSpec::Root(value)
    }
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
pub struct Root;

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

#[derive(Hash, Debug, Clone, Default, EnumString, IntoStaticStr)]
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
