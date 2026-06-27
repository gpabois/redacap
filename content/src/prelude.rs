use yrs::{ReadTxn, Transaction};

use crate::iter::ContentLeafs;

pub trait ContentDef: Sized {
    type NodeId: NodeId;
    type InitialNode;
    type Node: AsRef<str>;
}

pub trait RefContent<Cx>: ContentDef {
    fn root(&self) -> Self::NodeId;
    
    fn leafs<'a>(&'a self, cx: &'a Cx) -> ContentLeafs<'a, Cx, Self> {
        ContentLeafs::new(cx, self)
    }

    fn first_leaf_of(&self, cx: &Cx, id: Self::NodeId) -> Self::NodeId;
    fn last_leaf_of(&self, cx: &Cx, id: Self::NodeId) -> Self::NodeId;
    fn next_leaf_of(&self, cx: &Cx, id: Self::NodeId) -> Option<Self::NodeId>;

    fn parent_of(&self, cx: &Cx, id: Self::NodeId) -> Option<Self::NodeId>;
    fn children_of<'content>(&'content self, cx: &Cx, id: Self::NodeId) -> impl Iterator<Item = Self::NodeId> + 'content;
    fn ancestors_of<'content>(&'content self, cx: &Cx, id: Self::NodeId) -> impl Iterator<Item = Self::NodeId> + 'content;

    fn borrow<'content>(&'content self, id: Self::NodeId) -> &'content Self::Node;

}

pub trait MutContent<Cx>: ContentDef {
    fn borrow_mut<'content>(&'content self, cx: Cx, id: Self::NodeId) -> &'content Self::Node;
    fn add_child(&mut self, cx: Cx, parent: Self::Node, child: Self::NodeId, slot: ChildSlot<Self>);
    fn create_node<N>(&mut self, cx: Cx, data: N) -> Self::NodeId where Self::InitialNode: From<N>;
}

pub enum ChildSlot<Content: ContentDef> {
    Head,
    Tail,
    After(usize),
    AfterSibling(Content::NodeId),
    Before(usize),
    BeforeSibling(Content::NodeId)
}

pub trait NodeId: Copy {
    fn parent<Cx, Content>(self, cx: &Cx, content: &Content) -> Option<Self> 
        where Content: RefContent<Cx, NodeId = Self>
    {
        content.parent_of(cx, self)
    }

    fn children<Cx, Content>(self, cx: &Cx, content: &Content) -> impl Iterator<Item=Self> 
        where Content: RefContent<Cx, NodeId = Self>
    {
        content.children_of(cx, self)
    }
    
    fn next_leaf<Cx, Content>(self, cx: &Cx, content: &Content) -> Option<Self> 
        where Content: RefContent<Cx, NodeId = Self>
    {
        content.next_leaf_of(cx, self)
    }

    fn first_leaf<Cx, Content>(self, cx: &Cx, content: &Content) -> Self 
        where Content: RefContent<Cx, NodeId = Self>
    {
        content.first_leaf_of(cx, self)
    }
}

pub trait AsReadTxn {
    type Ref: ReadTxn;

    fn as_read_txn_ref(&self) -> &Self::Ref;
}

impl<'a, 'doc> AsReadTxn for &'a Transaction<'doc> {
    type Ref = Transaction<'doc>;

    fn as_read_txn_ref(&self) -> &Transaction<'doc> {
        self
    }
}