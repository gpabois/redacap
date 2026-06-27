use std::collections::HashMap;

use anyhow::{anyhow, bail};
use shared::id;
use yrs::{Array, ArrayPrelim, ArrayRef, Map, MapPrelim, MapRef, TextPrelim, TextRef, Transaction, TransactionMut, block::{ItemPtr, Prelim}};

use crate::{ContentId, Node, NodeKind, NodeSpec, prelude::{AsReadTxn, ContentDef, MutContent}};
pub struct ContentPrelim(yrs::MapPrelim);

impl ContentPrelim {
    pub fn new() -> Self {
        let prelim = MapPrelim::from([
            ("arena", ArrayPrelim::default()),
        ]);

        Self(prelim)
    }
}

impl Prelim for ContentPrelim {
    type Return = MapRef;

    fn into_content(self, txn: &mut yrs::TransactionMut) -> (yrs::block::ItemContent, Option<Self>) {
        let args = self.0.into_content(txn);
        (args.0, args.1.map(Self))
    }

    fn integrate(self, txn: &mut yrs::TransactionMut, inner_ref: yrs::branch::BranchPtr) {
        self.0.integrate(txn, inner_ref);
    }
}

pub struct ContentArenaRef(ArrayRef);

impl ContentArenaRef {
    pub fn iter<Cx: AsReadTxn>(&self, cx: &Cx) -> impl Iterator<Item=NodeRef> {
        self.0.iter(cx.as_read_txn_ref()).flat_map(|map| map.try_into())
    }
}

impl TryFrom<yrs::Out> for ContentArenaRef {
    type Error = anyhow::Error;

    fn try_from(value: yrs::Out) -> Result<Self, Self::Error> {
        let yrs::Out::YArray(array_ref) = value else { bail!("expecting an ArrayRef for ContentArenaRef") };
        Ok(Self(array_ref))
    }
}

pub struct ContentRef {
    inner: MapRef,
    arena: ContentArenaRef,
    index: HashMap<ContentId, NodeRef>
}

impl ContentDef for ContentRef {
    type NodeId = ContentId;
    type InitialNode = NodeSpec;
    type Node = NodeRef;
}

impl MutContent<&mut TransactionMut> for ContentRef {
    fn borrow_mut<'content>(&'content self, cx: &mut TransactionMut, id: Self::NodeId) -> &'content Self::Node {
        todo!()
    }

    fn add_child(&mut self, cx: &mut TransactionMut, parent: Self::Node, child: Self::NodeId, slot: crate::prelude::ChildSlot<Self>) {
        
    }

    fn create_node<N>(&mut self, cx: &mut TransactionMut, data: N) -> Self::NodeId where Self::InitialNode: From<N> {
        let id = ContentId::new();

        let node_ref = self.arena.0.push_back(
            txn, 
             Node {
                id: todo!(),
                spec: NodeSpec::from(data),
                ..default()
            }
        );

        self.index.insert(id, node_ref);
    }
}

impl ContentRef {
    fn load<Cx>(cx: &Cx, inner: MapRef) -> anyhow::Result<Self> where Cx: AsReadTxn {
        let tx = cx.as_read_txn_ref();
        
        let arena: ContentArenaRef = inner.get(tx, "arena")
            .ok_or(anyhow!("expecting 'arena' attribute in MapRef for ContentRef"))?
            .try_into()?;

        let index = arena
            .iter(cx)
            .map(|content_node| (content_node.content_id(cx), content_node))
            .collect();

        Ok(ContentRef {
            inner,
            arena, 
            index
        })
    }
}

#[derive(Clone)]
pub struct NodeRef(MapRef);

impl NodeRef {
    pub fn try_content_id<Cx>(&self, cx: &Cx) -> anyhow::Result<ContentId> where Cx: AsReadTxn {
        let id: String = self.0.get_as(cx.as_read_txn_ref(), "_id")?;
        let id: ContentId = id.parse()?;
        Ok(id)
    }

    pub fn content_id<Cx>(&self, cx: &Cx) -> ContentId where Cx: AsReadTxn {
        self.try_content_id(cx).unwrap()
    }
}

impl TryFrom<ItemPtr> for NodeRef {
    type Error = anyhow::Error;

    fn try_from(value: ItemPtr) -> Result<Self, Self::Error> {
        let map_ref = MapRef::try_from(value).unwrap();
        Ok(Self(map_ref))
    }
}

impl TryFrom<yrs::Out> for NodeRef {
    type Error = anyhow::Error;

    fn try_from(value: yrs::Out) -> Result<Self, Self::Error> {
        let yrs::Out::YMap(map_ref) = value else {bail!("expecting MapRef for ContentNodeRef")};
        Ok(Self(map_ref))
    }
}

impl Prelim for Node {
    type Return = NodeRef;

    fn into_content(self, txn: &mut yrs::TransactionMut) -> (yrs::block::ItemContent, Option<Self>) {
        let kind = NodeKind::from(&self.spec);

        let mut map  = HashMap::<&str, yrs::In>::new();

        map.insert("_id", self.id.into());
        map.insert("kind", kind.into());
        map.insert("parent", self.parent.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map.insert("next_sibling", self.next_sibling.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map.insert("prev_sibling", self.prev_sibling.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map.insert("spec", self.spec.into());

        let prelim = MapPrelim::from(map);
        let args = prelim.into_content(txn);
        (args.0, args.1.map(Self))
    }

    fn integrate(self, txn: &mut yrs::TransactionMut, inner_ref: yrs::branch::BranchPtr) {
        let map_ref = MapRef::from(inner_ref);

        map_ref.insert(txn, "id", self.id.into());
        map_ref.insert(txn, "parent", self.parent.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map_ref.insert(txn, "next_sibling", self.parent.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map_ref.insert(txn, "prev_sibling", self.parent.map(yrs::In::from).unwrap_or(yrs::In::Any(yrs::Any::Null)));
        map_ref.insert(txn, "spec", self.spec);

    }
}

#[derive(Clone)]
pub struct NodeSpecRef(MapRef);

impl Prelim for NodeSpec {
    type Return;

    fn into_content(self, txn: &mut yrs::TransactionMut) -> (yrs::block::ItemContent, Option<Self>) {
        let mut map  = HashMap::<&str, yrs::In>::new();
        let kind = NodeKind::from(&self);

        map.insert("kind", kind.into());

        match self {
            NodeSpec::Root => {},
            NodeSpec::Paragraph(paragraph) => {},
            NodeSpec::Plain(value) => {
                map.insert("text", TextPrelim::new(value).into());
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

        let prelim = MapPrelim::from(map);
        let args = prelim.into_content(txn);
        (args.0, args.1.map(Self))
    }

    fn integrate(self, txn: &mut yrs::TransactionMut, inner_ref: yrs::branch::BranchPtr) {
        let map_ref = MapRef::from(inner_ref);

        match self {
            NodeSpec::Root => {},
            NodeSpec::Paragraph(paragraph) => {},
            NodeSpec::Plain(value) => {
                map.insert(txn, "text", TextPrelim::new(value).into());
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
            NodeSpec::ListItem(list_item) => todo!(),
            NodeSpec::Table(table) => {},
            NodeSpec::Row(row) => {},
            NodeSpec::Cell(cell) => {},
        };
    }
}