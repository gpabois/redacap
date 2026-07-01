use anyhow::bail;
use bimap::BiHashMap;
use shared::id::IdGenerator;

use crate::{BodyNodeId, NodeKind, NodeSpec};
use crate::traits::node::{BodyRead, BodyWrite};

/// Backend "mode direct" : le corps de l'acte légal vit en mémoire locale,
/// sans CRDT. Les mutations sont immédiates et non synchronisées.
pub struct DirectBody {
    arena: indextree::Arena<NodeSpec>,
    index: BiHashMap<BodyNodeId, indextree::NodeId>,
    idgen: IdGenerator,
    root: BodyNodeId,
}

impl DirectBody {
    pub fn new() -> Self {
        let idgen = IdGenerator::default();
        let mut arena = indextree::Arena::new();
        let arena_id = arena.new_node(NodeSpec::Root);
        let root = BodyNodeId::from_raw(idgen.next_id());
        let mut index = BiHashMap::new();
        index.insert(root, arena_id);
        Self { arena, index, idgen, root }
    }

    fn arena_id_of(&self, id: BodyNodeId) -> indextree::NodeId {
        *self
            .index
            .get_by_left(&id)
            .unwrap_or_else(|| panic!("nœud inconnu : {id}"))
    }

    fn node_of(&self, id: BodyNodeId) -> &NodeSpec {
        self.arena.get(self.arena_id_of(id)).unwrap().get()
    }

    fn node_of_mut(&mut self, id: BodyNodeId) -> &mut NodeSpec {
        let aid = self.arena_id_of(id);
        self.arena.get_mut(aid).unwrap().get_mut()
    }
}

impl Default for DirectBody {
    fn default() -> Self {
        Self::new()
    }
}

impl BodyRead for DirectBody {
    fn root(&self) -> BodyNodeId {
        self.root
    }

    fn kind_of(&self, id: BodyNodeId) -> NodeKind {
        self.node_of(id).kind()
    }

    fn text_of(&self, id: BodyNodeId) -> String {
        match self.node_of(id) {
            NodeSpec::Plain(text) => text.clone(),
            _ => String::new(),
        }
    }

    fn parent_of(&self, id: BodyNodeId) -> Option<BodyNodeId> {
        let aid = self.arena_id_of(id);
        let parent_aid = self.arena.get(aid)?.parent()?;
        self.index.get_by_right(&parent_aid).copied()
    }

    fn children_of(&self, id: BodyNodeId) -> Vec<BodyNodeId> {
        let aid = self.arena_id_of(id);
        aid.children(&self.arena)
            .filter_map(|child_aid| self.index.get_by_right(&child_aid).copied())
            .collect()
    }

    fn spec_of(&self, id: BodyNodeId) -> NodeSpec {
        self.node_of(id).clone()
    }
}

impl BodyWrite for DirectBody {
    fn create_node(&mut self, spec: NodeSpec) -> BodyNodeId {
        let id = BodyNodeId::from_raw(self.idgen.next_id());
        let aid = self.arena.new_node(spec);
        self.index.insert(id, aid);
        id
    }

    fn insert_child_at_unchecked(
        &mut self,
        parent: BodyNodeId,
        index: usize,
        child: BodyNodeId,
    ) -> anyhow::Result<()> {
        let parent_aid = self.arena_id_of(parent);
        let child_aid = self.arena_id_of(child);
        match parent_aid.children(&self.arena).nth(index) {
            Some(at) => at.insert_before(child_aid, &mut self.arena),
            None => parent_aid.append(child_aid, &mut self.arena),
        }
        Ok(())
    }

    fn detach_unchecked(&mut self, id: BodyNodeId) -> anyhow::Result<()> {
        self.arena_id_of(id).detach(&mut self.arena);
        Ok(())
    }

    fn remove_subtree(&mut self, id: BodyNodeId) -> anyhow::Result<()> {
        let aid = self.arena_id_of(id);
        let removed: Vec<BodyNodeId> = aid
            .descendants(&self.arena)
            .filter_map(|d| self.index.get_by_right(&d).copied())
            .collect();
        aid.remove_subtree(&mut self.arena);
        for rid in removed {
            self.index.remove_by_left(&rid);
        }
        Ok(())
    }

    fn insert_text_unchecked(&mut self, id: BodyNodeId, char_index: usize, value: &str) {
        if let NodeSpec::Plain(text) = self.node_of_mut(id) {
            let byte = text
                .char_indices()
                .nth(char_index)
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            text.insert_str(byte, value);
        }
    }

    fn remove_text_unchecked(&mut self, id: BodyNodeId, char_index: usize, char_count: usize) {
        if let NodeSpec::Plain(text) = self.node_of_mut(id) {
            let mut indices = text.char_indices().map(|(i, _)| i);
            let Some(start) = indices.nth(char_index) else { return };
            let end = indices.nth(char_count.saturating_sub(1)).unwrap_or(text.len());
            text.replace_range(start..end, "");
        }
    }

    fn set_spec_unchecked(&mut self, id: BodyNodeId, spec: NodeSpec) -> anyhow::Result<()> {
        let current_kind = self.kind_of(id);
        let new_kind = spec.kind();
        if new_kind != current_kind {
            bail!(
                "impossible de remplacer un nœud {current_kind} par {new_kind} : genres différents"
            );
        }
        *self.node_of_mut(id) = spec;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use content::{List, ListItem, Span};

    use crate::kind::{Article, Chapitre, Titre};

    use super::*;

    fn new_body_with_article() -> (DirectBody, BodyNodeId) {
        let mut body = DirectBody::new();
        let article = body
            .append_node(body.root(), NodeSpec::Article(Article::default()))
            .unwrap();
        (body, article)
    }

    #[test]
    fn test_append_node_creates_label_and_plain() {
        let (body, article) = new_body_with_article();
        // L'article doit avoir un LibelleArticle créé par append_node + ensure_only_plain_leafs
        let children = body.children_of(article);
        assert!(
            children.iter().any(|&c| body.kind_of(c) == NodeKind::LibelleArticle),
            "LibelleArticle manquant"
        );
    }

    #[test]
    fn test_root_order_visa_before_titre() {
        let mut body = DirectBody::new();
        body.append_node(body.root(), NodeSpec::Titre(Titre::default())).unwrap();
        body.append_node(body.root(), NodeSpec::Visa).unwrap();

        let children = body.children_of(body.root());
        let groups: Vec<u8> = children
            .iter()
            .map(|&c| body.kind_of(c).root_order_group().unwrap())
            .collect();
        for w in groups.windows(2) {
            assert!(w[0] <= w[1], "ordre Root violé");
        }
    }

    #[test]
    fn test_annexe_always_last() {
        let mut body = DirectBody::new();
        body.append_node(body.root(), NodeSpec::Titre(Titre::default())).unwrap();
        body.append_node(body.root(), NodeSpec::Annexe(crate::kind::Annexe::default())).unwrap();
        body.append_node(body.root(), NodeSpec::Chapitre(Chapitre::default())).unwrap();

        let children = body.children_of(body.root());
        let last_kind = body.kind_of(*children.last().unwrap());
        assert_eq!(last_kind, NodeKind::Annexe, "l'Annexe doit être en dernier");
    }

    #[test]
    fn test_numbering_is_updated() {
        let mut body = DirectBody::new();
        body.append_node(body.root(), NodeSpec::Titre(Titre::default())).unwrap();
        body.append_node(body.root(), NodeSpec::Titre(Titre::default())).unwrap();
        body.append_node(body.root(), NodeSpec::Titre(Titre::default())).unwrap();

        let titres: Vec<BodyNodeId> = body
            .children_of(body.root())
            .into_iter()
            .filter(|&c| body.kind_of(c) == NodeKind::Titre)
            .collect();
        assert_eq!(titres.len(), 3);
        for (i, &t) in titres.iter().enumerate() {
            let num = body.spec_of(t).number().unwrap();
            assert_eq!(num, (i + 1) as u32, "numérotation incorrecte");
        }
    }

    #[test]
    fn test_remove_node_renumbers() {
        let mut body = DirectBody::new();
        let t1 = body.append_node(body.root(), NodeSpec::Titre(Titre::default())).unwrap();
        body.append_node(body.root(), NodeSpec::Titre(Titre::default())).unwrap();
        body.append_node(body.root(), NodeSpec::Titre(Titre::default())).unwrap();

        body.remove_node(t1).unwrap();

        let titres: Vec<BodyNodeId> = body
            .children_of(body.root())
            .into_iter()
            .filter(|&c| body.kind_of(c) == NodeKind::Titre)
            .collect();
        assert_eq!(titres.len(), 2);
        assert_eq!(body.spec_of(titres[0]).number().unwrap(), 1);
        assert_eq!(body.spec_of(titres[1]).number().unwrap(), 2);
    }

    #[test]
    fn test_cannot_remove_root() {
        let mut body = DirectBody::new();
        let err = body.remove_node(body.root()).unwrap_err();
        assert!(err.to_string().contains("Root"));
    }

    #[test]
    fn test_text_editing() {
        let (mut body, article) = new_body_with_article();
        // Trouver la première feuille (Plain sous LibelleArticle)
        let leaf = body.first_leaf_of(article);
        assert_eq!(body.kind_of(leaf), NodeKind::Plain);

        body.insert_text(leaf, 0, "hello");
        assert_eq!(body.text_of(leaf), "hello");

        body.insert_text(leaf, 5, " world");
        assert_eq!(body.text_of(leaf), "hello world");

        body.remove_text_unchecked(leaf, 0, 6);
        assert_eq!(body.text_of(leaf), "world");
    }

    #[test]
    fn test_split_plain() {
        let (mut body, article) = new_body_with_article();
        // Naviguer jusqu'au Paragraphe > Plain de l'article
        let paragraphe = body
            .children_of(article)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::Paragraphe)
            .unwrap();
        let plain = body.first_child_of(paragraphe).unwrap();

        body.insert_text(plain, 0, "hello world");
        let tail = body.split_node(plain, 5).unwrap();

        assert_eq!(body.text_of(plain), "hello");
        assert_eq!(body.text_of(tail), " world");
        assert_eq!(body.children_of(paragraphe), vec![plain, tail]);
    }

    #[test]
    fn test_leaf_navigation() {
        let (mut body, article) = new_body_with_article();
        let paragraphe = body
            .children_of(article)
            .into_iter()
            .find(|&c| body.kind_of(c) == NodeKind::Paragraphe)
            .unwrap();
        let plain = body.first_child_of(paragraphe).unwrap();

        body.insert_text(plain, 0, "a");
        let plain2 = body.split_node(plain, 1).unwrap();
        body.insert_text(plain2, 0, "b");

        assert_eq!(body.next_leaf_of(plain), Some(plain2));
        assert_eq!(body.prev_leaf_of(plain2), Some(plain));
    }

    #[test]
    fn test_invalid_child_rejected() {
        let mut body = DirectBody::new();
        // Plain ne peut pas être ajouté directement sous Root
        let err = body.append_node(body.root(), NodeSpec::Plain("x".into())).unwrap_err();
        assert!(err.to_string().contains("autorisé"));
    }
}
