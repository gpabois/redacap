use anyhow::bail;
use bimap::BiHashMap;
use shared::id::IdGenerator;

use crate::{ContentId, ContentKind, ContentRead, ContentWrite, NodeSpec};

/// Backend "mode direct" : l'arbre de contenu vit uniquement en mémoire
/// locale, sans CRDT. Les mutations sont immédiates et ne sont pas
/// synchronisées avec d'autres pairs.
pub struct DirectContent {
    arena: indextree::Arena<NodeSpec>,
    index: BiHashMap<ContentId, indextree::NodeId>,
    idgen: IdGenerator,
    root: ContentId,
}

impl DirectContent {
    pub fn new() -> Self {
        let idgen = IdGenerator::default();
        let mut arena = indextree::Arena::new();
        let arena_id = arena.new_node(NodeSpec::Root);

        let root = ContentId::from_raw(idgen.next_id());

        let mut index = BiHashMap::new();
        index.insert(root, arena_id);

        Self {
            arena,
            index,
            idgen,
            root,
        }
    }

    fn arena_id_of(&self, id: ContentId) -> indextree::NodeId {
        *self
            .index
            .get_by_left(&id)
            .unwrap_or_else(|| panic!("noeud de contenu inconnu : {id}"))
    }

    fn node_of(&self, id: ContentId) -> &NodeSpec {
        self.arena.get(self.arena_id_of(id)).unwrap().get()
    }

    fn node_of_mut(&mut self, id: ContentId) -> &mut NodeSpec {
        let arena_id = self.arena_id_of(id);
        self.arena.get_mut(arena_id).unwrap().get_mut()
    }
}

impl Default for DirectContent {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentRead for DirectContent {
    fn root(&self) -> ContentId {
        self.root
    }

    fn kind_of(&self, id: ContentId) -> ContentKind {
        self.node_of(id).kind()
    }

    fn text_of(&self, id: ContentId) -> String {
        match self.node_of(id) {
            NodeSpec::Plain(text) => text.clone(),
            _ => String::new(),
        }
    }

    fn parent_of(&self, id: ContentId) -> Option<ContentId> {
        let arena_id = self.arena_id_of(id);
        let parent_arena_id = self.arena.get(arena_id)?.parent()?;
        self.index.get_by_right(&parent_arena_id).copied()
    }

    fn children_of(&self, id: ContentId) -> Vec<ContentId> {
        let arena_id = self.arena_id_of(id);
        arena_id
            .children(&self.arena)
            .filter_map(|child_arena_id| self.index.get_by_right(&child_arena_id).copied())
            .collect()
    }

    fn spec_of(&self, id: ContentId) -> NodeSpec {
        self.node_of(id).clone()
    }
}

impl ContentWrite for DirectContent {
    fn create_node<N>(&mut self, spec: N) -> ContentId
    where
        NodeSpec: From<N>,
    {
        let id = ContentId::from_raw(self.idgen.next_id());
        let arena_id = self.arena.new_node(NodeSpec::from(spec));
        self.index.insert(id, arena_id);
        id
    }

    fn insert_child_at(
        &mut self,
        parent: ContentId,
        index: usize,
        child: ContentId,
    ) -> anyhow::Result<()> {
        let parent_arena_id = self.arena_id_of(parent);
        let child_arena_id = self.arena_id_of(child);

        match parent_arena_id.children(&self.arena).nth(index) {
            Some(at_arena_id) => at_arena_id.insert_before(child_arena_id, &mut self.arena),
            None => parent_arena_id.append(child_arena_id, &mut self.arena),
        }

        Ok(())
    }

    fn detach_unchecked(&mut self, id: ContentId) -> anyhow::Result<()> {
        self.arena_id_of(id).detach(&mut self.arena);
        Ok(())
    }

    fn remove_node(&mut self, id: ContentId) -> anyhow::Result<()> {
        let arena_id = self.arena_id_of(id);

        let removed = arena_id
            .descendants(&self.arena)
            .filter_map(|descendant_arena_id| {
                self.index.get_by_right(&descendant_arena_id).copied()
            })
            .collect::<Vec<_>>();

        arena_id.remove_subtree(&mut self.arena);

        for removed_id in removed {
            self.index.remove_by_left(&removed_id);
        }

        Ok(())
    }

    fn insert_text(&mut self, id: ContentId, char_index: usize, value: &str) {
        if let NodeSpec::Plain(text) = self.node_of_mut(id) {
            let byte_index = text
                .char_indices()
                .nth(char_index)
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            text.insert_str(byte_index, value);
        }
    }

    fn remove_text(&mut self, id: ContentId, char_index: usize, char_count: usize) {
        if let NodeSpec::Plain(text) = self.node_of_mut(id) {
            let mut indices = text.char_indices().map(|(i, _)| i);
            let Some(start) = indices.nth(char_index) else {
                return;
            };
            let end = indices
                .nth(char_count.saturating_sub(1))
                .unwrap_or(text.len());
            text.replace_range(start..end, "");
        }
    }

    fn set_spec(&mut self, id: ContentId, spec: NodeSpec) -> anyhow::Result<()> {
        let current_kind = self.kind_of(id);
        let new_kind = spec.kind();

        if new_kind != current_kind {
            bail!(
                "impossible de remplacer un noeud {current_kind} par un noeud {new_kind} : les genres diffèrent"
            );
        }

        *self.node_of_mut(id) = spec;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Cell, ContentKind as Kind, List, ListItem, Paragraph, Row, Span, Table};

    use super::*;

    #[test]
    fn test_create_compatible_node() {
        // Plain > Paragraph > Root
        let mut body = DirectContent::new();
        let id = body.append_content(body.root, "").unwrap();
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::Paragraph, Kind::Root]);

        // Span > Paragraph > Root
        let mut body = DirectContent::new();
        let id = body.append_content(body.root, Span::default()).unwrap();
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::Paragraph, Kind::Root]);

        // ListItem > List > Root
        let mut body = DirectContent::new();
        let id = body.append_content(body.root, ListItem::default()).unwrap();
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::List, Kind::Root]);

        // Cell > Row > Table > Root
        let mut body = DirectContent::new();
        let id = body.append_content(body.root, Cell).unwrap();
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::Row, Kind::Table, Kind::Root]);

        // List > Root, Row > Table > Root, Table > Root (déjà compatibles : aucun noeud créé)
        let mut body = DirectContent::new();
        let id = body.append_content(body.root, List::default()).unwrap();
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::Root]);

        let mut body = DirectContent::new();
        let id = body.append_content(body.root, Row).unwrap();
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::Table, Kind::Root]);

        let mut body = DirectContent::new();
        let id = body.append_content(body.root, Table).unwrap();
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();
        assert_eq!(got.as_slice(), &[Kind::Root]);
    }

    #[test]
    fn test_only_plain_leafs() {
        // Plain > Span > Paragraph > Root
        let mut body = DirectContent::new();
        body.append_content(body.root, Span::default()).unwrap();
        let id = body.first_leaf_of(body.root());
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();

        assert_eq!(body.kind_of(id), Kind::Plain);
        assert_eq!(got.as_slice(), &[Kind::Span, Kind::Paragraph, Kind::Root]);

        // Plain > ListItem > List > Root
        let mut body = DirectContent::new();
        body.append_content(body.root, List::default()).unwrap();
        let id = body.first_leaf_of(body.root());
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();

        assert_eq!(body.kind_of(id), Kind::Plain);
        assert_eq!(got.as_slice(), &[Kind::ListItem, Kind::List, Kind::Root]);

        // Plain > Cell > Row > Table > Root
        let mut body = DirectContent::new();
        body.append_content(body.root, Table).unwrap();
        let id = body.first_leaf_of(body.root());
        let got = body
            .ancestors_of(id)
            .into_iter()
            .map(|id| body.kind_of(id))
            .collect::<Vec<_>>();

        assert_eq!(body.kind_of(id), Kind::Plain);
        assert_eq!(
            got.as_slice(),
            &[Kind::Cell, Kind::Row, Kind::Table, Kind::Root]
        );
    }

    #[test]
    fn test_text_editing() {
        let mut body = DirectContent::new();
        let id = body.append_content(body.root, "hello").unwrap();

        body.insert_text(id, 5, " world");
        assert_eq!(body.text_of(id), "hello world");

        body.remove_text(id, 0, 6);
        assert_eq!(body.text_of(id), "world");
    }

    #[test]
    fn test_leaf_navigation() {
        let mut body = DirectContent::new();
        let first = body.append_content(body.root, "a").unwrap();
        let second = body.append_content(body.root, "b").unwrap();

        assert_eq!(body.next_leaf_of(first), Some(second));
        assert_eq!(body.prev_leaf_of(second), Some(first));
        assert_eq!(
            body.leaf_order_of(first, second),
            Some(std::cmp::Ordering::Less)
        );
    }

    #[test]
    fn test_spec_of_and_set_spec() {
        let mut body = DirectContent::new();
        let id = body.create_node(Span {
            bold: true,
            ..Span::default()
        });

        let NodeSpec::Span(span) = body.spec_of(id) else {
            panic!("attendu un Span")
        };
        assert!(span.bold);
        assert!(!span.italic);

        body.set_spec(
            id,
            NodeSpec::Span(Span {
                italic: true,
                ..Span::default()
            }),
        )
        .unwrap();
        let NodeSpec::Span(span) = body.spec_of(id) else {
            panic!("attendu un Span")
        };
        assert!(!span.bold);
        assert!(span.italic);

        let err = body.set_spec(id, NodeSpec::Plain("x".into())).unwrap_err();
        assert!(err.to_string().contains("genres diffèrent"));
    }

    #[test]
    fn test_merge_with_prev_text() {
        let mut body = DirectContent::new();
        let paragraph = body.create_node(Paragraph);
        body.insert_child_at(body.root, 0, paragraph).unwrap();

        let first = body.create_node("hello ");
        let second = body.create_node("world");
        body.insert_child_at(paragraph, 0, first).unwrap();
        body.insert_child_at(paragraph, 1, second).unwrap();

        let merged = body.merge_with_prev(second).unwrap();
        assert_eq!(merged, first);
        assert_eq!(body.text_of(first), "hello world");
        assert_eq!(body.children_of(paragraph), vec![first]);
    }

    #[test]
    fn test_merge_with_prev_kind_mismatch() {
        let mut body = DirectContent::new();
        let paragraph = body.create_node(Paragraph);
        body.insert_child_at(body.root, 0, paragraph).unwrap();

        let plain = body.create_node("x");
        let span = body.create_node(Span::default());
        body.insert_child_at(paragraph, 0, plain).unwrap();
        body.insert_child_at(paragraph, 1, span).unwrap();

        let err = body.merge_with_prev(span).unwrap_err();
        assert!(err.to_string().contains("structures incompatibles"));
    }

    #[test]
    fn test_merge_into_paragraph_and_list_item() {
        let mut body = DirectContent::new();
        let paragraph = body.create_node(Paragraph);
        let list_item = body.create_node(ListItem::default());
        body.insert_child_at(body.root, 0, paragraph).unwrap();
        body.insert_child_at(body.root, 1, list_item).unwrap();

        let plain = body.create_node("hello");
        let span = body.create_node(Span::default());
        body.insert_child_at(list_item, 0, plain).unwrap();
        body.insert_child_at(list_item, 1, span).unwrap();

        body.merge_into(paragraph, list_item).unwrap();

        assert_eq!(body.kind_of(paragraph), Kind::Paragraph);
        assert_eq!(body.children_of(paragraph), vec![plain, span]);
        assert_eq!(body.children_of(body.root), vec![paragraph]);
    }

    #[test]
    fn test_split_node_text() {
        let mut body = DirectContent::new();
        let id = body.append_content(body.root, "hello world").unwrap();
        let paragraph = body.parent_of(id).unwrap();

        let tail = body.split_node(id, 5).unwrap();
        assert_eq!(body.text_of(id), "hello");
        assert_eq!(body.text_of(tail), " world");
        assert_eq!(body.children_of(paragraph), vec![id, tail]);
    }

    #[test]
    fn test_split_node_container() {
        let mut body = DirectContent::new();
        let paragraph = body.create_node(Paragraph);
        body.insert_child_at(body.root, 0, paragraph).unwrap();

        let a = body.create_node("a");
        let b = body.create_node("b");
        let c = body.create_node("c");
        body.insert_child_at(paragraph, 0, a).unwrap();
        body.insert_child_at(paragraph, 1, b).unwrap();
        body.insert_child_at(paragraph, 2, c).unwrap();

        let new_paragraph = body.split_node(paragraph, 1).unwrap();
        assert_eq!(body.children_of(paragraph), vec![a]);
        assert_eq!(body.children_of(new_paragraph), vec![b, c]);
        assert_eq!(body.children_of(body.root), vec![paragraph, new_paragraph]);
        assert_eq!(body.kind_of(new_paragraph), Kind::Paragraph);
    }
}
