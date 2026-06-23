use std::{cmp::Ordering, collections::HashMap, str::FromStr};

use anyhow::anyhow;
use bimap::{BiHashMap};
use strum_macros::{AsRefStr, EnumDiscriminants};

use crate::utils::{ID, IdGenerator};

#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub content_id: ContentId,
    /// Offset exprimé en nombre de char (et non en bytes)
    pub offset: usize
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.content_id, self.offset)
    }
}

impl Cursor {
    pub fn into_byte_offset<S: AsRef<str>>(self, value: S) -> Option<usize> {
        value.as_ref().char_indices().nth(self.offset).map(|(i,_)| i)
    }

    pub fn split_clone<S: AsRef<str>>(self, value: S) -> (String, String) {
        let value = value.as_ref();

        let Some(index) = self.into_byte_offset(value) else {
            return (value.to_owned(), String::default());
        };

        if value.len() <= self.offset {
            return (value.to_owned(), String::default());
        }

        let (lhs, rhs) = value.split_at(index);
        (lhs.to_owned(), rhs.to_owned())   
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Selection {
    pub anchor: Cursor,
    pub focus: Cursor
}

impl Cursor {
    pub fn partial_cmp(&self, rhs: &Cursor, body: &ContentBody) -> Option<Ordering> {
        if self.content_id == rhs.content_id {
            return self.offset.partial_cmp(&rhs.offset)
        }

        self.content_id.leaf_order(rhs.content_id, body)
    }
    pub fn is_content_within(&self, content_id: ContentId) -> bool {
        self.content_id == content_id
    }

    pub fn left(&mut self, body: &ContentBody) {
        if self.offset == 0 && let Some(content_id) = self.content_id.prev_leaf(&body) {
            self.content_id = content_id;
            self.offset = content_id.len(body);
        } else if self.offset > 0 {
            self.offset -= 1;
        }
    }

    pub fn right(&mut self, body: &ContentBody) {
        if self.offset >= self.content_id.len(body) && let Some(content_id) = self.content_id.next_leaf(body) {
            self.content_id = content_id;
            self.offset = content_id.len(body);
        } else if self.offset <= self.content_id.len(body) {
            self.offset += 1;
        }
    }
}

pub struct ContentLeafs<'body> {
    current: Option<ContentId>,
    body: &'body ContentBody
}

impl<'body> ContentLeafs<'body> {
    pub fn new(body: &'body ContentBody) -> Self {
        Self {
            current: Some(body.first_leaf_of(body.root)),
            body
        }
    }
}

impl<'body> Iterator for ContentLeafs<'body> {
    type Item = ContentId;

    fn next(&mut self) -> Option<Self::Item> {
        let leaf_id = self.current?;
        self.current = leaf_id.next_leaf(self.body);
        Some(leaf_id)
    }
}

pub struct ContentBody {
    arena: indextree::Arena<Content>,
    index: BiHashMap<ContentId, indextree::NodeId>,
    /// Leaf-order index
    loi: HashMap<ContentId, usize>,
    idgen: IdGenerator,
    pub root: ContentId 
}

impl ContentBody {
    pub fn new() -> Self {
        let idgen = IdGenerator::new();
        let mut arena: indextree::Arena<Content> = indextree::Arena::new();
        let mut index = BiHashMap::default();
        let arena_id = arena.new_node(Content::Root);
        let root = ContentId(idgen.next_id());
        let loi = HashMap::new();
        index.insert(root, arena_id);
        Self {root, arena, index, idgen, loi}
    }

    fn leaf_order(&self, lhs: ContentId, rhs: ContentId) -> Option<Ordering> {
        let lhs = *self.loi.get(&lhs)?;
        let rhs = *self.loi.get(&rhs)?;

        lhs.partial_cmp(&rhs)
    }

    fn create_unchecked_node<C>(&mut self, content: C) -> ContentId where Content: From<C> {
        let id = ContentId(self.idgen.next_id());
        let node = Content::from(content);
        let arena_id = self.arena.new_node(node);
        self.index.insert(id, arena_id);
        id
    }
    
    fn rebuild_loi(&mut self) {
        let leafs = ContentLeafs::new(self).enumerate().collect::<Vec<_>>();
        self.loi = leafs.into_iter().map(|(i, id)| (id, i)).collect();
    }

    fn ensure_only_plain_leafs(&mut self) -> anyhow::Result<()> {
        use ContentKind::Plain;

        let leafs = ContentLeafs::new(self).collect::<Vec<_>>();

        for leaf in leafs.into_iter() {
            match leaf.kind(self) {
                Plain => {},
                _ => {
                    let plain_id = self.create_unchecked_node("");
                    let compat_id = self.ensure_compatible_node_for(leaf, plain_id)?;
                    self.append_unchecked_child(leaf, compat_id)?;
                }
            }
        }

        Ok(())
    }

    fn ensure_compatible_node_for(&mut self, target: ContentId, descendant: ContentId) -> anyhow::Result<ContentId> {
        let parent_kind = target.kind(self);
        let kind = descendant.kind(self);
        let mut content_id = descendant;
        let mut path = kind
            .correction_path(parent_kind)
            .ok_or(anyhow!("noeud de contenu enfant incompatible avec le parent {kind} vs. {parent_kind}"))?;
        path.reverse();

        for content in path.into_iter().map(ContentKind::new_default_content) {
            let parent_id = self.create_unchecked_node::<Content>(content);
            self.append_unchecked_child(parent_id, content_id)?;
            content_id = parent_id;
        }

        Ok(content_id)
    }

    fn append_unchecked_child(&mut self, parent: ContentId, child: ContentId) -> anyhow::Result<()> {
        let parent_arena_id = self.index.get_by_left(&parent).copied().unwrap();
        let child_arena_id = self.index.get_by_left(&child).copied().unwrap();
        parent_arena_id.append(child_arena_id, &mut self.arena);       
        Ok(())
    }

    fn append_child(&mut self, parent: ContentId, child: ContentId) -> anyhow::Result<()> {
        self.append_unchecked_child(parent, child)?;
        self.ensure_only_plain_leafs()?;
        self.rebuild_loi();
        Ok(())
    }

    fn first_leaf_of(&self, id: ContentId) -> ContentId {
        let mut current = id;
        
        while let Some(first_child) = self.first_child_of(current) {
            current = first_child;
        }

        current
    }

    fn last_leaf_of(&self, id: ContentId) -> ContentId {
        let mut current = id;
        
        while let Some(first_child) = self.last_child_of(current) {
            current = first_child;
        }

        current
    }

    fn prev_leaf_of(&self, id: ContentId) -> Option<ContentId> {
        for ancestor in self.ancestors_of(id) {
            // Si l'ancêtre a un frère précédent, on a trouvé notre embranchement à gauche
            if let Some(sibling) = self.prev_sibling_of(ancestor) {
                let mut current = sibling;

                while let Some(last_child) = self.last_child_of(current) {
                    current = last_child;
                }

                return Some(current)
            }
        }

        None
    }

    fn next_leaf_of(&self, id: ContentId) -> Option<ContentId> {
        for ancestor in self.ancestors_of(id) {
            // Si l'ancêtre a un frère précédent, on a trouvé notre embranchement à gauche
            if let Some(sibling) = self.next_sibling_of(ancestor) {
                let mut current = sibling;

                while let Some(first_child) = self.first_child_of(current) {
                    current = first_child;
                }

                return Some(current)
            }
        }

        None
    }

    fn ancestors_of<'body>(&'body self, id: ContentId) -> impl Iterator<Item=ContentId> + 'body {
        let arena_id = self.index.get_by_left(&id).copied().unwrap();
        arena_id.ancestors(&self.arena)
        .flat_map(|node_id| self.index.get_by_right(&node_id).copied())
        .skip(1)
    }

    fn parent_of(&self, id: ContentId) -> Option<ContentId> {
        let node = self.try_get(id)?;
        let arena_id = node.parent()?;
        self.index.get_by_right(&arena_id).copied()
    }

    fn prev_sibling_of(&self, id: ContentId) -> Option<ContentId> {
        let node = self.try_get(id)?;
        let arena_id = node.previous_sibling()?;
        self.index.get_by_right(&arena_id).copied()
    }

    fn next_sibling_of(&self, id: ContentId) -> Option<ContentId> {
        let node = self.try_get(id)?;
        let arena_id = node.next_sibling()?;
        self.index.get_by_right(&arena_id).copied()
    }

    fn last_child_of(&self, id: ContentId) -> Option<ContentId> {
        self.children_of(id).last()
    }

    fn first_child_of(&self, id: ContentId) -> Option<ContentId> {
        self.children_of(id).next()
    }

    fn children_of<'body>(&'body self, id: ContentId) -> impl Iterator<Item=ContentId> + 'body {
        let arena_id = self.index.get_by_left(&id).copied().unwrap();
        arena_id
            .children(&self.arena)
            .flat_map(|arena_id| self.index.get_by_right(&arena_id).copied())
    }

    fn try_get(&self, id: ContentId) -> Option<&indextree::Node<Content>> {
        let arena_id = *self.index.get_by_left(&id)?;
        self.arena.get(arena_id)
    }

    fn try_get_mut(&mut self, id: ContentId) -> Option<&mut indextree::Node<Content>> {
        let arena_id = *self.index.get_by_left(&id)?;
        self.arena.get_mut(arena_id)
    }
}

#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentId(ID);

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ContentId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}

impl ContentId {
    pub fn children<'body>(self, body: &'body ContentBody) -> impl Iterator<Item=ContentId> + 'body {
        body.children_of(self)
    }

    pub fn leaf_order(self, rhs: Self, body: &ContentBody) -> Option<Ordering> {
        body.leaf_order(self, rhs)
    }

    pub fn parent(self, body: &ContentBody) -> Option<ContentId> {
        body.parent_of(self)
    }

    pub fn ancestors(self, body: &ContentBody) -> impl Iterator<Item=ContentId> {
        body.ancestors_of(self)
    }

    pub fn kind(&self, body: &ContentBody) -> ContentKind {
        ContentKind::from(self.borrow(body))
    }

    pub fn borrow(self, body: &ContentBody) -> &Content {
        body.try_get(self).unwrap().get()
    }

    pub fn borrow_mut(self, body: &mut ContentBody) -> &mut Content {
        body.try_get_mut(self).unwrap().get_mut()
    }

    #[inline]
    pub fn first_leaf(self, body: &ContentBody) -> ContentId {
        body.first_leaf_of(self)
    }

    #[inline]
    pub fn last_leaf(self, body: &ContentBody) -> ContentId {
        body.last_leaf_of(self)
    }

    #[inline]
    pub fn prev_leaf(self, body: &ContentBody) -> Option<ContentId> {
        body.prev_leaf_of(self)
    }

    #[inline]
    pub fn next_leaf(self, body: &ContentBody) -> Option<ContentId> {
        body.next_leaf_of(self)
    }

    pub fn append_child(self, child_id: ContentId, body: &mut ContentBody) -> anyhow::Result<()> {
        body.append_child(self, child_id)
    }

    pub fn append_content<C>(self, content: C, body: &mut ContentBody) -> anyhow::Result<ContentId>  where Content: From<C> {
        let content_id = body.create_unchecked_node(content);
        let compat_id = body.ensure_compatible_node_for(self, content_id)?;
        self.append_child(compat_id, body)?;
        Ok(content_id)
    }

    pub fn len(&self, body: &ContentBody) -> usize {
        self.borrow(&body).len()
    }
}

#[derive(Hash, Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(derive(AsRefStr), name(ContentKind))]pub enum Content {
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

impl AsRef<str> for Content {
    fn as_ref(&self) -> &str {
        if let Self::Plain(text) = self {
            text.as_ref()
        } else {
            ""
        }
    }
}

impl Content {
    /// Split le texte au point fixé par le curseur
    /// 
    /// Si le curseur est OOB, le droit sera vide.
    pub fn split_clone_text_at(&self, cursor: &Cursor) -> (String, String) {
        cursor.split_clone(self)
    }
}

impl std::fmt::Display for ContentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl ContentKind {
    pub fn new_default_content(self) -> Content {
        match self {
            ContentKind::Root => Content::Root,
            ContentKind::Paragraph => Paragraph::default().into(),
            ContentKind::Plain => Content::Plain(String::default()),
            ContentKind::Span => Content::Span(Span::default()),
            ContentKind::List => Content::List(List::default()),
            ContentKind::ListItem => Content::ListItem(ListItem::default()),
            ContentKind::Table => Content::Table(Table::default()),
            ContentKind::Row => Content::Row(Row::default()),
            ContentKind::Cell => Content::Cell(Cell::default()),
        }
    }

    #[inline]
    pub fn can_accept_child(self, child: Self) -> bool {
        self.allowed_children().contains(&child)
    }
    
    pub fn correction_path(self, to: Self) -> Option<Vec<ContentKind>> {
        let mut pth = self.find_ascending_path(to, vec![])?;

        pth.pop();

        Some(pth)
    }

    pub const TABLE: &[(Self, &[Self])] = &[
        (Self::Root,        &[Self::Paragraph, Self::List, Self::Table]),
        (Self::Paragraph,   &[Self::Plain, Self::Span]),
        (Self::Span,        &[Self::Plain, Self::Span]),
        (Self::List,        &[Self::ListItem]),
        (Self::ListItem,    &[Self::Span, Self::Plain]),
        (Self::Table,       &[Self::Row]),
        (Self::Row,         &[Self::Cell]),
        (Self::Cell,        &[Self::Span, Self::Plain]),
        (Self::Plain,       &[])
    ];

    pub fn find_ascending_path(self, to: Self, mut visited: Vec<ContentKind>) -> Option<Vec<ContentKind>> {
        visited.push(self);
        if self.parents().any(|par| par == to) {
            return Some(vec![self]);
        }

        let mut iter = self.parents()
            .filter(|par| !visited.contains(par))
            .flat_map(|par| par.find_ascending_path(to, visited.clone()))
            .collect::<Vec<_>>();

        iter.sort_by(|a,b| a.len().cmp(&b.len()));

        for mut pth in iter {
            pth.push(self);
            return Some(pth)
        }

        None
    }

    #[inline]
    pub fn allowed_children(self) -> &'static [ContentKind] {
        Self::TABLE.iter().find(|(kind, _)| self == *kind).unwrap().1
    }

    pub fn parents(self) -> impl Iterator<Item = ContentKind> {
        Self::TABLE.iter()
            .filter(move |(_, children)| children.contains(&self))
            .map(|(parent, _)| *parent)
    }

    pub fn default_parent(self) -> Option<ContentKind> {
        pub use ContentKind::*;
        
        Some(match self {
            Row => Table,
            Cell => Row,
            ListItem => List,
            Paragraph => Root,
            Table => Root,
            List => Root,
            Span => Paragraph,
            Plain => Paragraph,
            _ => return None
        })
    }
}

impl Content {
    pub fn len(&self) -> usize {
        match self{
            Self::Plain(str) => str.chars().count(),
            _ => 0
        }
    }
}

impl From<&str> for Content {
    fn from(value: &str) -> Self {
        Self::Plain(value.to_string())
    }
}

impl From<Paragraph> for Content {
    fn from(value: Paragraph) -> Self {
        Content::Paragraph(value)
    }
}

impl From<List> for Content {
    fn from(value: List) -> Self {
        Content::List(value)
    }
}

impl From<ListItem> for Content {
    fn from(value: ListItem) -> Self {
        Content::ListItem(value)
    }
}

impl From<Span> for Content {
    fn from(value: Span) -> Self {
        Content::Span(value)
    }
}

impl From<Table> for Content {
    fn from(value: Table) -> Self {
        Content::Table(value)
    }
}

impl From<Row> for Content {
    fn from(value: Row) -> Self {
        Content::Row(value)
    }
}

impl From<Cell> for Content {
    fn from(value: Cell) -> Self {
        Content::Cell(value)
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

#[cfg(test)]
mod tests {
use crate::model::content::{Cell, ContentBody, ContentKind, List, ListItem, Paragraph, Row, Span, Table};

    #[test]
    pub fn test_correction_path() {
        use ContentKind::*;
        
        let got = Plain.correction_path(Root).unwrap(); 
        assert_eq!(got.as_slice(), &[Paragraph]);

        let got = Span.correction_path(Root).unwrap(); 
        assert_eq!(got.as_slice(), &[Paragraph]);

        let got = Paragraph.correction_path(Root).unwrap(); 
        assert_eq!(got.as_slice(), &[]);

        let got = Cell.correction_path(Root).unwrap(); 
        assert_eq!(got.as_slice(), &[Table, Row]);

        let got = Row.correction_path(Root).unwrap(); 
        assert_eq!(got.as_slice(), &[Table]);

        let got = Table.correction_path(Root).unwrap(); 
        assert_eq!(got.as_slice(), &[]);
    }

    #[test]
    pub fn test_create_compatible_node() {
        use ContentKind as Kind;

        // Plain > Paragraph > Root
        let mut body = ContentBody::new();
        let id = body.root.append_content("", &mut body).unwrap();
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(got.as_slice(), &[Kind::Paragraph, Kind::Root]);
        
        // Span > Paragraph > Root
        let mut body = ContentBody::new();
        let id = body.root.append_content(Span::default(), &mut body).unwrap();
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(got.as_slice(), &[Kind::Paragraph, Kind::Root]);

        // Paragraph > Root
        let mut body = ContentBody::new();
        let id = body.root.append_content(Paragraph::default(), &mut body).unwrap();
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(got.as_slice(), &[Kind::Root]);

        // ListItem > List > Root
        let mut body = ContentBody::new();
        let id = body.root.append_content(ListItem::default(), &mut body).unwrap();
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(got.as_slice(), &[Kind::List, Kind::Root]);

        // List > Root
        let mut body = ContentBody::new();
        let id = body.root.append_content(List::default(), &mut body).unwrap();
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(got.as_slice(), &[Kind::Root]);

        // Cell > Row > Table > Root
        let mut body = ContentBody::new();
        let id = body.root.append_content(Cell::default(), &mut body).unwrap();
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(got.as_slice(), &[Kind::Row, Kind::Table, Kind::Root]);

        // Row > Table > Root
        let mut body = ContentBody::new();
        let id = body.root.append_content(Row::default(), &mut body).unwrap();
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(got.as_slice(), &[Kind::Table, Kind::Root]);

        // Table > Root
        let mut body = ContentBody::new();
        let id = body.root.append_content(Table::default(), &mut body).unwrap();
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(got.as_slice(), &[Kind::Root]);

    }

    #[test]
    pub fn test_only_plain_leafs() {
        use ContentKind as Kind;

        // Plain > Span > Paragraph > Root
        let mut body = ContentBody::new();
        body.root.append_content(Span::default(), &mut body).unwrap();
        let id = body.root.first_leaf(&body);
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(id.kind(&body), Kind::Plain);
        assert_eq!(got.as_slice(), &[Kind::Span, Kind::Paragraph, Kind::Root]);

        // Plain >  Paragraph > Root
        let mut body = ContentBody::new();
        body.root.append_content(Paragraph::default(), &mut body).unwrap();
        let id = body.root.first_leaf(&body);
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(id.kind(&body), Kind::Plain);
        assert_eq!(got.as_slice(), &[Kind::Paragraph, Kind::Root]);

        // Plain >  ListItem > List > Root
        let mut body = ContentBody::new();
        body.root.append_content(List::default(), &mut body).unwrap();
        let id = body.root.first_leaf(&body);
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(id.kind(&body), Kind::Plain);
        assert_eq!(got.as_slice(), &[Kind::ListItem, Kind::List, Kind::Root]);

        // Plain >  Cell > Row > Table > Root
        let mut body = ContentBody::new();
        body.root.append_content(Table::default(), &mut body).unwrap();
        let id = body.root.first_leaf(&body);
        let got = id.ancestors(&body)
            .map(|id| id.kind(&body))
            .collect::<Vec<_>>();

        assert_eq!(id.kind(&body), Kind::Plain);
        assert_eq!(got.as_slice(), &[Kind::Cell, Kind::Row, Kind::Table, Kind::Root]);
        
    }
}