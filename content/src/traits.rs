use crate::{ListMarker, MutNodeSpec, NodeKind, RefNodeSpec, iter::ContentLeafs};

pub trait ReadableNodeSpecDef<'a> {
    type RefRoot: ReadableRoot + 'a;
    type RefParagraph: ReadableParagraph + 'a;
    type RefSpan: ReadableSpan + 'a;
    type RefPlain: ReadablePlain + 'a;
    type RefList: ReadableList + 'a;
    type RefListItem: ReadableListItem + 'a;
    type RefTable: ReadableTable + 'a;
    type RefRow: ReadableRow + 'a;
    type RefCell: ReadableCell + 'a;
}

pub trait WritableNodeSpecDef<'a> {
    type MutRoot: WritableRoot + 'a;
    type MutParagraph: WritableParagraph + 'a;
    type MutSpan: WritableSpan + 'a;    
    type MutPlain: WritablePlain + 'a;
    type MutList: WritableList + 'a;
    type MutListItem: WritableListItem + 'a;
    type MutTable: WritableTable + 'a;
    type MutRow: WritableRow + 'a;
    type MutCell: WritableCell + 'a;
}


pub trait ContentDef: Sized {
    type NodeId: NodeId;
    type InitialNode;
}

pub trait ReadableContent<'a>: ContentDef {
    type NodeSpecDef: ReadableNodeSpecDef<'a>;

    fn root(&self) -> Self::NodeId;
    
    fn leafs(&'a self) -> ContentLeafs<'a, Self> {
        ContentLeafs::new(self)
    }

    fn first_leaf_of(&self, id: Self::NodeId) -> Self::NodeId;
    fn last_leaf_of(&self, id: Self::NodeId) -> Self::NodeId;
    fn next_leaf_of(&self, id: Self::NodeId) -> Option<Self::NodeId>;

    fn kind_of(&self, id: Self::NodeId) -> NodeKind;
    fn parent_of(&self, id: Self::NodeId) -> Option<Self::NodeId>;
    fn children_of<'content>(&'content self, id: Self::NodeId) -> impl Iterator<Item = Self::NodeId> + 'content;
    fn ancestors_of<'content>(&'content self, id: Self::NodeId) -> impl Iterator<Item = Self::NodeId> + 'content;

    fn read(&self, id: Self::NodeId) -> RefNodeSpec<Self::NodeSpecDef>;
}

pub trait WritableContent<'a>: ContentDef {
    type NodeSpecDef: WritableNodeSpecDef<'a>;

    fn add_child(&mut self, parent: Self::NodeId, child: Self::NodeId, slot: ChildSlot<Self>);
    fn create_node<N>(&mut self, data: N) -> Self::NodeId where Self::InitialNode: From<N>;

     fn write(&self, id: Self::NodeId) -> MutNodeSpec<Self::NodeSpecDef>;
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
    fn kind<'a, Content>(self, content: &'a Content) -> NodeKind 
        where Content: ReadableContent<'a, NodeId = Self>
    {
        content.kind_of(self)
    }

    fn parent<'a, Content>(self, content: &'a Content) -> Option<Self> 
        where Content: ReadableContent<'a, NodeId = Self>
    {
        content.parent_of(self)
    }

    fn children<'a, Content>(self, content: &'a Content) -> impl Iterator<Item=Self> 
        where Content: ReadableContent<'a, NodeId = Self>
    {
        content.children_of(self)
    }
    
    fn next_leaf<'a, Content>(self, content: &'a Content) -> Option<Self> 
        where Content: ReadableContent<'a, NodeId = Self>
    {
        content.next_leaf_of( self)
    }

    fn first_leaf<'a, Content>(self, content: &'a Content) -> Self 
        where Content: ReadableContent<'a, NodeId = Self>
    {
        content.first_leaf_of(self)
    }
}

pub trait ReadableRoot {}
pub trait WritableRoot {}

pub trait ReadableParagraph {}
pub trait WritableParagraph {}

pub trait ReadablePlain { 
    fn text(&self) -> String;
}
pub trait WritablePlain {
    fn insert_text<S>(&mut self, index: u32, value: S) where S: AsRef<str>;
    fn append_text<S>(&mut self, value: S) where S: AsRef<str>;
    fn replace_text<S>(&mut self, value: S) where S: AsRef<str>;
}

pub trait ReadableSpan {
    fn italic(&self) -> bool;
    fn bold(&self) -> bool;
    fn underline(&self) -> bool;    
    fn striekout(&self) -> bool;
}
pub trait WritableSpan {
    fn set_bold(&mut self, value: bool);
    fn set_italic(&mut self, value: bool);
    fn set_underline(&mut self, value: bool);
    fn set_strikeout(&mut self, value: bool);
}

pub trait ReadableList {}
pub trait WritableList {}

pub trait ReadableListItem {
    fn marker(&self) -> ListMarker;
}
pub trait WritableListItem {
    fn set_marker(&mut self, marker: ListMarker);
}

pub trait ReadableTable {}
pub trait WritableTable {}

pub trait ReadableRow {}
pub trait WritableRow {}

pub trait ReadableCell {}
pub trait WritableCell {}

