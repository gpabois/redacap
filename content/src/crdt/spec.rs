use core::panic;
use std::{collections::HashMap, str::FromStr};

use yrs::{GetString, Map, ReadTxn, Text, TransactionMut};

use crate::{NodeKind, NodeSpec, prelude::ReadableSpan, traits};

impl From<NodeSpec> for yrs::MapPrelim {
    fn from(value: NodeSpec) -> yrs::MapPrelim {
        let mut map  = HashMap::<&str, yrs::In>::new();
        let kind = NodeKind::from(&value);

        map.insert("kind", kind.into());

        match value {
            NodeSpec::Root(root) => {},
            NodeSpec::Paragraph(paragraph) => {},
            NodeSpec::Plain(value) => {
                map.insert("text", yrs::TextPrelim::new(value).into());
            },
            NodeSpec::Span(span) => {
                map.insert("bold", span.bold.into());
                map.insert("italic", span.italic.into());
                map.insert("underline", span.underline.into());
                map.insert("strikeout", span.strikeout.into());
            },
            NodeSpec::List(list) => {
                map.insert("marker", list.marker.into());
                map.insert("start", list.start.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
            },
            NodeSpec::ListItem(list_item) => {},
            NodeSpec::Table(table) => {},
            NodeSpec::Row(row) => {},
            NodeSpec::Cell(cell) => {},
        };

        yrs::MapPrelim::from_iter(map.into_iter())
    }
}

impl From<NodeSpec> for yrs::In {
    fn from(value: NodeSpec) -> Self {
        yrs::In::Map(value.into())
    }
}

#[derive(Clone)]
pub struct NodeSpecRef(yrs::MapRef);

#[derive(Debug, Clone)]
pub struct RootRef(yrs::MapRef);

impl RootRef {
    pub fn read<'a, Txn>(&'a self, txn: &'a Txn) -> ReadTxRoot<'a, Txn> where Txn: ReadTxn {
        ReadTxRoot {r#ref: self, txn}
    }

    pub fn write<'a>(&'a self, txn: &'a mut TransactionMut<'a>) -> WriteTxRoot<'a> {
        WriteTxRoot {r#ref: self, txn}
    }
}

pub struct ReadTxRoot<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a RootRef
}

pub struct WriteTxRoot<'a> {
    txn : &'a mut TransactionMut<'a>,
    r#ref: &'a RootRef
}

impl<'a, Txn> traits::ReadableRoot for ReadTxRoot<'a, Txn> where Txn: ReadTxn {}
impl<'a> traits::ReadableRoot for WriteTxRoot<'a> {}
impl<'a> traits::WritableRoot for WriteTxRoot<'a> {}

#[derive(Debug, Clone)]
pub struct ParagraphRef(yrs::MapRef);

pub struct ReadTxParagraph<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a ParagraphRef
}

pub struct WriteTxParagraph<'a> {
    txn : &'a mut TransactionMut<'a>,
    r#ref: &'a ParagraphRef
}

impl<'a, Txn> traits::ReadableParagraph for ReadTxParagraph<'a, Txn> where Txn: ReadTxn {}
impl<'a> traits::ReadableParagraph for WriteTxParagraph<'a> {}
impl<'a> traits::WritableParagraph for WriteTxParagraph<'a> {}


#[derive(Debug, Clone)]
pub struct SpanRef(yrs::MapRef);

impl SpanRef {
    pub fn read<'a, Txn>(&'a self, txn: &'a Txn) -> ReadTxSpan<'a, Txn> where Txn: ReadTxn {
        ReadTxSpan {
            txn,
            r#ref: self
        }
    }

    pub fn write<'a>(&'a self, txn: &'a mut TransactionMut<'a>) -> WriteTxSpan<'a> {
        WriteTxSpan { txn, r#ref: self }
    }
}

pub struct ReadTxSpan<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a SpanRef
}

pub struct WriteTxSpan<'a> {
    txn: &'a mut TransactionMut<'a>,
    r#ref: &'a SpanRef
}

impl<'a, Txn> traits::ReadableSpan for ReadTxSpan<'a, Txn> where Txn: ReadTxn {
    fn italic(&self) -> bool {
        use yrs::Out::Any;
        use yrs::Any::Bool;
        let Some(Any(Bool(value))) = self.r#ref.0.get(self.txn, "italic") else { panic!("expecting italic field to be a boolean field in SpanRef") };
        value
    }

    fn bold(&self) -> bool {
        use yrs::Out::Any;
        use yrs::Any::Bool;
        let Some(Any(Bool(value))) = self.r#ref.0.get(self.txn, "bold") else { panic!("expecting bold field to be a boolean field in SpanRef") };
        value
    }

    fn underline(&self) -> bool {
        use yrs::Out::Any;
        use yrs::Any::Bool;
        let Some(Any(Bool(value))) = self.r#ref.0.get(self.txn, "underline") else { panic!("expecting underline field to be a boolean field in SpanRef") };
        value
    }

    fn striekout(&self) -> bool {
        use yrs::Out::Any;
        use yrs::Any::Bool;
        let Some(Any(Bool(value))) = self.r#ref.0.get(self.txn, "strikeout") else { panic!("expecting strikeout field to be a boolean field in SpanRef") };
        value
    }
}
impl<'a> traits::ReadableSpan for WriteTxSpan<'a> {
    fn italic(&self) -> bool {
        use yrs::Out::Any;
        use yrs::Any::Bool;
        let Some(Any(Bool(value))) = self.r#ref.0.get(self.txn, "italic") else { panic!("expecting italic field to be a boolean field in SpanRef") };
        value
    }

    fn bold(&self) -> bool {
        use yrs::Out::Any;
        use yrs::Any::Bool;
        let Some(Any(Bool(value))) = self.r#ref.0.get(self.txn, "bold") else { panic!("expecting bold field to be a boolean field in SpanRef") };
        value
    }

    fn underline(&self) -> bool {
        use yrs::Out::Any;
        use yrs::Any::Bool;
        let Some(Any(Bool(value))) = self.r#ref.0.get(self.txn, "underline") else { panic!("expecting underline field to be a boolean field in SpanRef") };
        value
    }

    fn striekout(&self) -> bool {
        use yrs::Out::Any;
        use yrs::Any::Bool;
        let Some(Any(Bool(value))) = self.r#ref.0.get(self.txn, "strikeout") else { panic!("expecting strikeout field to be a boolean field in SpanRef") };
        value
    }
}
impl<'a> traits::WritableSpan for WriteTxSpan<'a> {
    fn set_bold(&mut self, value: bool) {
        self.r#ref.0.try_update(self.txn, "bold", value);
    }

    fn set_italic(&mut self, value: bool) {
        self.r#ref.0.try_update(self.txn, "italic", value);
    }

    fn set_underline(&mut self, value: bool) {
        self.r#ref.0.try_update(self.txn, "underline", value);
    }

    fn set_strikeout(&mut self, value: bool) {
        self.r#ref.0.try_update(self.txn, "strikeout", value);
    }
}


#[derive(Debug, Clone)]
pub struct ListRef(yrs::MapRef);

pub struct ReadTxList<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a ListRef
}

pub struct WriteTxList<'a> {
    txn : &'a mut TransactionMut<'a>,
    r#ref: &'a ListRef
}

impl<'a, Txn> traits::ReadableList for ReadTxList<'a, Txn> where Txn: ReadTxn {}
impl<'a> traits::ReadableList for WriteTxList<'a> {}
impl<'a> traits::WritableList for WriteTxList<'a> {}

#[derive(Debug, Clone)]
pub struct ListItemRef(yrs::MapRef);

pub struct ReadTxListItem<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a ListItemRef
}

pub struct WriteTxListItem<'a> {
    txn : &'a mut TransactionMut<'a>,
    r#ref: &'a ListItemRef
}

impl<'a, Txn> traits::ReadableListItem for ReadTxListItem<'a, Txn> where Txn: ReadTxn {
    fn marker(&self) -> crate::ListMarker {
        let marker_str: &str = self.r#ref.0.get_as(self.txn, "marker").unwrap();
        crate::ListMarker::from_str(marker_str).unwrap()
    }
}
impl<'a> traits::ReadableListItem for WriteTxListItem<'a> {
    fn marker(&self) -> crate::ListMarker {
        let marker_str: &str = self.r#ref.0.get_as(self.txn, "marker").unwrap();
        crate::ListMarker::from_str(marker_str).unwrap()
    }
}
impl<'a> traits::WritableListItem for WriteTxListItem<'a> {
    fn set_marker(&mut self, marker: crate::ListMarker) {
        let marker_str = marker.to_string();
        self.r#ref.0.try_update(self.txn, "marker", marker_str);
    }
}


#[derive(Debug, Clone)]
pub struct TableRef(yrs::MapRef);

pub struct ReadTxTable<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a TableRef
}

pub struct WriteTxTable<'a> {
    txn : &'a mut TransactionMut<'a>,
    r#ref: &'a TableRef
}

impl<'a, Txn> traits::ReadableTable for ReadTxTable<'a, Txn> where Txn: ReadTxn { }
impl<'a> traits::ReadableTable for WriteTxTable<'a> { }
impl<'a> traits::WritableTable for WriteTxTable<'a> { }

#[derive(Debug, Clone)]
pub struct RowRef(yrs::MapRef);

pub struct ReadTxRow<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a RowRef
}

pub struct WriteTxRow<'a> {
    txn : &'a mut TransactionMut<'a>,
    r#ref: &'a RowRef
}

impl<'a, Txn> traits::ReadableRow for ReadTxRow<'a, Txn> where Txn: ReadTxn { }
impl<'a> traits::ReadableRow for WriteTxRow<'a> { }
impl<'a> traits::WritableRow for WriteTxRow<'a> { }

#[derive(Debug, Clone)]
pub struct CellRef(yrs::MapRef);

pub struct ReadTxCell<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a CellRef
}

pub struct WriteTxCell<'a> {
    txn : &'a mut TransactionMut<'a>,
    r#ref: &'a CellRef
}

impl<'a, Txn> traits::ReadableCell for ReadTxCell<'a, Txn> where Txn: ReadTxn { }
impl<'a> traits::ReadableCell for WriteTxCell<'a> { }
impl<'a> traits::WritableCell for WriteTxCell<'a> { }

pub struct PlainRef(yrs::TextRef);

pub struct ReadTxPlain<'a, Txn> where Txn: ReadTxn {
    txn: &'a Txn,
    r#ref: &'a PlainRef
}

pub struct WriteTxPlain<'a> {
    txn : &'a mut TransactionMut<'a>,
    r#ref: &'a PlainRef
}

impl<'a, Txn> traits::ReadablePlain for ReadTxPlain<'a, Txn> where Txn: ReadTxn {
    fn text(&self) -> String {
        self.r#ref.0.get_string(self.txn)
    }
}
impl<'a> traits::ReadablePlain for WriteTxPlain<'a> {
    fn text(&self) -> String {
        self.r#ref.0.get_string(self.txn)
    }
}
impl<'a> traits::WritablePlain for WriteTxPlain<'a> {
    fn insert_text<S>(&mut self, index: u32, chunk: S) where S: AsRef<str> {
        self.r#ref.0.insert(self.txn, index, chunk.as_ref());
    }

    fn append_text<S>(&mut self, chunk: S) where S: AsRef<str> {
        self.r#ref.0.push(self.txn, chunk.as_ref());
    }

    fn replace_text<S>(&mut self, value: S) where S: AsRef<str> {
        self.r#ref.0.remove_range(self.txn, 0, self.r#ref.0.len(self.txn));
        self.r#ref.0.push(self.txn, value.as_ref());
    }   
}
