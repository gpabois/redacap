pub mod content;
pub mod node;
pub mod spec;
pub mod arena;

use std::{collections::HashMap, ops::Deref};
use anyhow::{anyhow, bail};
use yrs::{Array, ArrayRef, Map, MapRef, ReadTxn, TransactionMut};

use crate::{ContentId, ListMarker, NodeKind, NodeSpec, prelude::{ContentDef, ReadableContent, ReadableNodeSpecDef, WritableContent}, traits::WritableNodeSpecDef};

impl TryFrom<yrs::Out> for ContentId {
    type Error = yrs::Out;

    fn try_from(value: yrs::Out) -> Result<Self, Self::Error> {
        let yrs::Out::Any(yrs::Any::Buffer(bytes)) = value else { return Err(value) };
        Self::try_from(bytes.deref()).map_err(|_| value)
    }
}


impl From<NodeKind> for yrs::In {
    fn from(value: NodeKind) -> Self {
        let value: &str = value.into();
        yrs::any!(value)
    }
}

impl From<ContentId> for yrs::Out {
    fn from(value: ContentId) -> Self {
        yrs::any!(value.as_bytes())
    }
}

impl From<ContentId> for yrs::In {
    fn from(value: ContentId) -> Self {
        yrs::any!(value.as_bytes())
    }
}


pub struct ContentArenaRef(ArrayRef);

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
    index: HashMap<ContentId, node::NodeRef>,
    kinds: HashMap<ContentId, NodeKind>
}

pub struct ReadTxnContent<'a, Txn> where Txn: ReadTxn {
    r#ref: &'a ContentRef,
    txn: &'a Txn
}

pub struct WriteTxnContent<'a> {
    r#ref: &'a ContentRef,
    txn: &'a mut TransactionMut<'a>
}

impl<'a, Txn> ContentDef for ReadTxnContent<'a, Txn> where Txn: ReadTxn{
    type NodeId = ContentId;
    type InitialNode = NodeSpec;
}

impl<'a, Txn> ReadableNodeSpecDef<'a> for ReadTxnContent<'a, Txn> where Txn: ReadTxn {
    type RefRoot = spec::ReadTxRoot<'a, Txn>;
    type RefParagraph = spec::ReadTxParagraph<'a, Txn>;
    type RefSpan = spec::ReadTxSpan<'a, Txn>;
    type RefPlain = spec::ReadTxPlain<'a, Txn>;
    type RefList = spec::ReadTxList<'a, Txn>;
    type RefListItem = spec::ReadTxListItem<'a, Txn>;
    type RefTable = spec::ReadTxTable<'a, Txn>;
    type RefRow = spec::ReadTxRow<'a, Txn>;
    type RefCell = spec::ReadTxCell<'a, Txn>;
}

impl<'a> ReadableNodeSpecDef<'a> for WriteTxnContent<'a> {
    type RefRoot = spec::WriteTxRoot<'a>;
    type RefParagraph = spec::WriteTxParagraph<'a>;
    type RefSpan = spec::WriteTxSpan<'a>;
    type RefPlain = spec::WriteTxPlain<'a>;
    type RefList = spec::WriteTxList<'a>;
    type RefListItem = spec::WriteTxListItem<'a>;
    type RefTable = spec::WriteTxTable<'a>;
    type RefRow = spec::WriteTxRow<'a>;
    type RefCell = spec::WriteTxCell<'a>;
}

impl<'a> WritableNodeSpecDef<'a> for WriteTxnContent<'a> {
    type MutRoot = spec::WriteTxRoot<'a>;
    type MutParagraph = spec::WriteTxParagraph<'a>;
    type MutSpan = spec::WriteTxSpan<'a>;
    type MutPlain = spec::WriteTxPlain<'a>;
    type MutList = spec::WriteTxList<'a>;
    type MutListItem = spec::WriteTxListItem<'a>;
    type MutTable = spec::WriteTxTable<'a>;
    type MutRow = spec::WriteTxRow<'a>;
    type MutCell = spec::WriteTxCell<'a>;
}

fn root<Txn>(content: &ContentRef, txn: &Txn) -> ContentId where Txn: ReadTxn {
    let out = content.inner.get(txn, "root").unwrap();
    let yrs::Out::Any(yrs::Any::Buffer(buf)) = out else { panic!("expecting root to be a bytes buffer") };
    let binding = buf.deref();
    let bytes = binding.deref();
    ContentId::try_from(bytes).unwrap()
}

fn kind_of(content: &ContentRef, id: ContentId) -> NodeKind {
    content.kinds.get(&id).copied().unwrap()
}

impl<'a, Txn> ReadableContent<'a> for ReadTxnContent<'a, Txn> where Txn: ReadTxn {
    type NodeSpecDef = Self;

    fn root(&self) -> Self::NodeId {
        root(self.r#ref, self.txn)
    }

    fn kind_of(&self, id: Self::NodeId) -> NodeKind {
        kind_of(self.r#ref, id)
    }

    fn first_leaf_of(&self, id: Self::NodeId) -> Self::NodeId {
    
    }

    fn last_leaf_of(&self, id: Self::NodeId) -> Self::NodeId {
        todo!()
    }

    fn next_leaf_of(&self, id: Self::NodeId) -> Option<Self::NodeId> {
        todo!()
    }

    fn parent_of(&self, id: Self::NodeId) -> Option<Self::NodeId> {
        todo!()
    }

    fn children_of<'content>(&'content self, id: Self::NodeId) -> impl Iterator<Item = Self::NodeId> + 'content {
        let (txn, ctx) = &self;
        let txn = txn.as_read_txn_ref();

    }

    fn ancestors_of<'content>(&'content self, id: Self::NodeId) -> impl Iterator<Item = Self::NodeId> + 'content {
        todo!()
    }
    

    
    fn read(&self, id: Self::NodeId) -> crate::RefNodeSpec<Self::NodeSpecDef> {
        todo!()
    }
}

impl<Txn> WritableContent for (Txn, ContentRef) where Txn: AsMutTxn {

    fn add_child(&mut self, cx: Cx, parent: Self::Node, child: Self::NodeId, slot: crate::prelude::ChildSlot<Self>) {
        
    }

    fn create_node<N>(&mut self, cx: &mut TransactionMut, data: N) -> Self::NodeId where Self::InitialNode: From<N> {
        let (txn, content) = &mut self;
        let id = ContentId::new();
        let kind = NodeKind::from(&data);

        let node_ref = content.arena.0.push_back(
            cx, 
             NodePrelim::new(id, data)
        );

        content.index.insert(id, node_ref);
        content.kinds.insert(id, kind);
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






impl From<ListMarker> for yrs::In {
    fn from(value: ListMarker) -> Self {
        let value: &str = value.into();
        yrs::any!(value)
    }
}