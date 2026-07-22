use std::ops::Deref;
use anyhow::bail;
use loro::{ContainerID, ContainerTrait, LoroDoc, LoroMap, LoroValue, UpdateOptions, ValueOrContainer, loro_value};
use shared::id;
use crate::data::{NodeData, NodeKind};

use crate::id::NodeId;


#[derive(Clone)]
pub struct LegalActProject(pub(crate) LoroDoc);

impl Deref for LegalActProject {
    type Target = LoroDoc;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl LegalActProject {
    pub fn new() -> Self {
        use NodeKind::{VisaRoot, ConsiderantRoot, SurRoot, BodyRoot, CommentRoot};
        let value = Self(LoroDoc::new());
        value.arena().insert(Node::new("visas", VisaRoot));
        value.arena().insert(Node::new("considerants", ConsiderantRoot));
        value.arena().insert(Node::new("sur", SurRoot));
        value.arena().insert(Node::new("body", BodyRoot));
        value.arena().insert(Node::new("comments", CommentRoot));
        value
    }

    pub fn visas(&self) -> NodeId {
        NodeId::from("visas")
    }

    pub fn considerants(&self) -> NodeId {
        NodeId::from("considerants")
    }

    pub fn sur(&self) -> NodeId {
        NodeId::from("sur")
    }

    pub fn body(&self) -> NodeId {
        NodeId::from("body")
    }
    
    pub fn comments(&self) -> NodeId {
        NodeId::from("comments")
    }

    pub fn node(&self, id: &NodeId) -> Option<Node> {
        self.arena().get(id)
    }

    pub fn title(&self) -> String {
        self.0.get_text("title").to_string()
    }

    pub fn set_title(&self, title: impl ToString) {
        let _ = self.0.get_text("title").update(&title.to_string(), UpdateOptions::default());
    }

    pub fn kind(&self, id: &NodeId) -> NodeKind {
        let node = self.arena().get(id).unwrap_or_else(|| panic!("nœud inconnu : {id}"));
        let data = node.data().unwrap_or_else(|| panic!("données invalides pour le nœud {id}"));
        NodeKind::from(&data)
    }

    /// Retourne le texte du nœud, ou `String::default()` si son type n'a pas de champ texte.
    pub fn text(&self, id: &NodeId) -> String {
        self.arena().get(id)
            .and_then(|node| node.data())
            .map(|data| data.text())
            .unwrap_or_default()
    }

    pub(crate) fn create_node(&self, data: impl Into<NodeData>) -> NodeId {
        let id = NodeId::new();
        let node = Node::new(id, data);
        self.arena().insert(node);
        id
    }

    pub fn children_of(&self, node: NodeId) -> Vec<NodeId> {
        let Some(parent) = self.arena().get(&node) else { return vec![] };
        parent.children()
    }

    /// Retourne la première feuille du sous-arbre à partir de `id`.
    pub fn first_leaf_of(&self, id: &NodeId) -> Option<NodeId> {
        let mut node = self.arena().get(id)?;

        loop {
            match node.children().into_iter().next() {
                Some(child) => node = self.arena().get(&child)?,
                None => return Some(node.id()),
            }
        }
    }

    /// Retourne la dernière feuille du sous-arbre à partir de `id`.
    pub fn last_leaf_of(&self, id: &NodeId) -> Option<NodeId> {
        let mut node = self.arena().get(id)?;

        loop {
            match node.last_child() {
                Some(child) => node = self.arena().get(&child)?,
                None => return Some(node.id()),
            }
        }
    }

    /// Retourne la feuille suivant `id` (qui doit être une feuille) dans l'ordre du document.
    pub fn next_leaf(&self, id: &NodeId) -> Option<NodeId> {
        let mut node = self.arena().get(id)?;

        loop {
            if let Some(next_sibling) = node.next_sibling() {
                return self.first_leaf_of(&next_sibling);
            }

            node = self.arena().get(&node.parent()?)?;
        }
    }

    /// Retourne la feuille précédant `id` (qui doit être une feuille) dans l'ordre du document.
    pub fn prev_leaf(&self, id: &NodeId) -> Option<NodeId> {
        let mut node = self.arena().get(id)?;

        loop {
            if let Some(prev_sibling) = node.prev_sibling() {
                return self.last_leaf_of(&prev_sibling);
            }

            node = self.arena().get(&node.parent()?)?;
        }
    }

    /// Retourne un itérateur sur les feuilles du sous-arbre à partir de `from`.
    pub fn leafs(&self, from: &NodeId) -> Leafs {
        Leafs {
            project: self.clone(),
            root: *from,
            next: self.first_leaf_of(from),
        }
    }

    /// Comme [`Self::next_leaf`], mais s'arrête sans dépasser `root`.
    fn next_leaf_within(&self, id: &NodeId, root: &NodeId) -> Option<NodeId> {
        let mut node = self.arena().get(id)?;

        loop {
            if &node.id() == root {
                return None;
            }

            if let Some(next_sibling) = node.next_sibling() {
                return self.first_leaf_of(&next_sibling);
            }

            node = self.arena().get(&node.parent()?)?;
        }
    }

    /// Retourne la première soeur de `id` (en remontant la chaîne des soeurs précédentes).
    pub fn first_sibling_of(&self, id: &NodeId) -> Option<NodeId> {
        let mut node = self.arena().get(id)?;

        loop {
            match node.prev_sibling() {
                Some(prev) => node = self.arena().get(&prev)?,
                None => return Some(node.id()),
            }
        }
    }

    /// Retourne la dernière soeur de `id` (en descendant la chaîne des soeurs suivantes).
    pub fn last_sibling_of(&self, id: &NodeId) -> Option<NodeId> {
        let mut node = self.arena().get(id)?;

        loop {
            match node.next_sibling() {
                Some(next) => node = self.arena().get(&next)?,
                None => return Some(node.id()),
            }
        }
    }

    /// Retourne la soeur suivant directement `id`.
    pub fn next_sibling_of(&self, id: &NodeId) -> Option<NodeId> {
        self.arena().get(id)?.next_sibling()
    }

    /// Retourne la soeur précédant directement `id`.
    pub fn prev_sibling_of(&self, id: &NodeId) -> Option<NodeId> {
        self.arena().get(id)?.prev_sibling()
    }

    /// Retourne un itérateur sur les soeurs de `from`, à partir de la première.
    pub fn siblings(&self, from: &NodeId) -> Siblings {
        Siblings {
            project: self.clone(),
            next: self.first_sibling_of(from),
        }
    }

    /// Détache le noeud de son parent et de ses soeurs
    pub fn detach(&self, node: &NodeId) {
        self.detach_from_parent(node);
        self.detach_from_siblings(node);
    }

    /// Ajoute un enfant à la fin de la liste des enfants
    pub fn append_child(&self, parent: &NodeId, child: &NodeId) {
        self.detach(child); // on assure que le noeud est bien détaché

        let Some(child_node) = self.arena().get(child) else { return };
        let Some(parent_node) = self.arena().get(parent) else { return };

        child_node.set_parent(parent_node.id());

        if let Some(last_child) = parent_node.last_child() {
            self.link_siblings(&last_child, child);
        }

        parent_node.append_child(child_node.id());
    }

    /// Ajoute un enfant à une position donnée
    pub fn insert_child(&self, parent: &NodeId, child: &NodeId, position: usize) {
        self.detach(child); // on assure que le noeud est bien détaché

        let Some(child_node) = self.arena().get(child) else { return };
        let Some(parent_node) = self.arena().get(parent) else { return };

        child_node.set_parent(parent_node.id());

        let position = position.min(parent_node.children().len());

        if position > 0 && let Some(prev) = parent_node.nth_child(position - 1) {
            self.link_siblings(&prev, child);
        }

        if let Some(next) = parent_node.nth_child(position) {
            self.link_siblings(child, &next);
        }

        parent_node.insert_child(child_node.id(), position);
    }

    fn detach_from_parent(&self, node: &NodeId) {
        let Some(child) = self.arena().get(node) else { return };
        let Some(parent_id) = child.parent() else { return };
        let Some(parent) = self.arena().get(&parent_id) else { return };

        child.remove_parent();
        parent.remove_child(&child.id());
    }

    fn detach_from_siblings(&self, node: &NodeId) {
        let Some(node) = self.arena().get(node) else { return };

        match (node.prev_sibling(), node.next_sibling()) {
            (None, None) => {},
            (None, Some(next)) => self.remove_prev_sibling(&next),
            (Some(prev), None) => self.remove_next_sibling(&prev),
            (Some(prev), Some(next)) => self.link_siblings(&prev, &next),
        }

        self.remove_prev_sibling(&node.id());
        self.remove_next_sibling(&node.id());
    }


    fn remove_prev_sibling(&self, sibling: &NodeId) {
        let Some(sibling) = self.arena().get(sibling) else { return };
        sibling.remove_prev_sibling();
    }

    fn remove_next_sibling(&self, sibling: &NodeId) {
        let Some(sibling) = self.arena().get(sibling) else { return };
        sibling.remove_next_sibling();
    }

    /// Lie deux noeuds adjacents entre eux : `prev.next = next` et `next.prev = prev`.
    fn link_siblings(&self, prev: &NodeId, next: &NodeId) {
        let Some(prev_node) = self.arena().get(prev) else { return };
        let Some(next_node) = self.arena().get(next) else { return };

        prev_node.set_next_sibling(*next);
        next_node.set_prev_sibling(*prev);
    }

    /// Divise `node_id` en position `pos`, et remonte récursivement dans les ancêtres
    /// (en les divisant à leur tour) jusqu'à ce que le parent courant soit de type `split_up_to`.
    ///
    /// Retourne l'identifiant du dernier noeud créé (celui dont le parent est `split_up_to`).
    pub fn split_at(&self, node_id: &NodeId, pos: usize, split_up_to: NodeKind) -> Option<NodeId> {
        let node = self.arena().get(node_id)?;
        let parent_id = node.parent()?;
        let parent = self.arena().get(&parent_id)?;

        let data = node.data()?;

        let new_id = if let NodeData::Plain(text) = &data {
            let byte_at = text.char_indices().nth(pos).map(|(i, _)| i).unwrap_or(text.len());
            let head = text[..byte_at].to_string();
            let tail = text[byte_at..].to_string();

            node.set_data(NodeData::Plain(head));
            self.create_node(NodeData::Plain(tail))
        } else {
            let new_id = self.create_node(data);

            for child in node.children().into_iter().skip(pos) {
                self.append_child(&new_id, &child);
            }

            new_id
        };

        let position = parent.children().iter().position(|c| c == node_id)? + 1;
        self.insert_child(&parent_id, &new_id, position);

        if self.kind(&parent_id) == split_up_to {
            Some(new_id)
        } else {
            self.split_at(&parent_id, position, split_up_to)
        }
    }

    fn arena(&self) -> Arena {
        Arena(self.0.get_map("arena"))
    }
}

/// Itérateur sur les feuilles d'un sous-arbre, dans l'ordre du document.
pub struct Leafs {
    project: LegalActProject,
    root: NodeId,
    next: Option<NodeId>,
}

impl Iterator for Leafs {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let leaf = self.next.take()?;
        self.next = self.project.next_leaf_within(&leaf, &self.root);
        Some(leaf)
    }
}

/// Itérateur sur les soeurs d'un noeud, dans l'ordre du document.
pub struct Siblings {
    project: LegalActProject,
    next: Option<NodeId>,
}

impl Iterator for Siblings {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let sibling = self.next.take()?;
        self.next = self.project.next_sibling_of(&sibling);
        Some(sibling)
    }
}

pub struct Node(LoroMap);

impl From<Node> for LoroValue {
    fn from(value: Node) -> Self {
        value.0.into()
    }
}

impl Node {
    pub(crate) fn new(id: impl Into<NodeId>, data: impl Into<NodeData>) -> Self {
        let id = id.into();
        let data = data.into();
        let data = serde_json::to_value(data).unwrap();

        let value: LoroMap = loro_value!({
            "id": id.as_bytes(),
            "data": data,
            "prev_sibling": None,
            "next_sibling": None,
            "parent": None,
            "children": [],
        }).into();

        Self(value)
    }

    pub fn id(&self) -> NodeId {
        let bytes = self.0.get("id").unwrap().as_value().unwrap().as_binary().unwrap();
        NodeId::try_from(bytes).unwrap()
    }

    pub fn container_id(&self) -> ContainerID {
        self.0.to_container().id()
    }

    fn next_sibling(&self) -> Option<NodeId> {
        use ValueOrContainer::Value;
        let Some(Value(value)) = self.0.get("next_sibling") else { return None };
        NodeId::try_from(value).ok()
    }   

    fn set_next_sibling(&self, next_sibling: NodeId) {
        self.0.insert("next_sibling", next_sibling);
    }

    fn remove_next_sibling(&self) {
        self.0.insert("next_sibling", None);
    }

    fn prev_sibling(&self) -> Option<NodeId> {
        use ValueOrContainer::Value;
        let Some(Value(value)) = self.0.get("prev_sibling") else { return None };
        NodeId::try_from(value).ok()
    }   

    fn set_prev_sibling(&self, prev_sibling: NodeId) {
        self.0.insert("prev_sibling", prev_sibling);
    }

    fn remove_prev_sibling(&self) {
        self.0.insert("prev_sibling", None);
    }

    fn parent(&self) -> Option<NodeId> {
        use ValueOrContainer::Value;
        let Some(Value(value)) = self.0.get("parent") else { return None };
        NodeId::try_from(value).ok()
    }   

    fn remove_parent(&self) {
        self.0.insert("parent", None);
    }

    fn remove_child(&self, child: &NodeId) {
        let Some(pos) = self.children().iter().position(|c| c == child) else { return };

        use ValueOrContainer::Container;
        let Some(Container(container)) = self.0.get("children") else { return };
        let Ok(list) = container.into_list() else { return };

        let _ = list.delete(pos, 1);
    }

    fn set_parent(&self, parent: NodeId) {
        self.0.insert("parent", parent);
    }

    pub fn data(&self) -> Option<NodeData> {
        let value = self.0.get("data")?.get_deep_value();
        let json: serde_json::Value = value.into();
        serde_json::from_value(json).ok()
    }

    fn set_data(&self, data: impl Into<NodeData>) {
        let data = data.into();
        let json = serde_json::to_value(data).unwrap();
        self.0.insert("data", json);
    }

    fn assert_position(&self, pos: usize) -> usize {
        pos.min(self.children().len() - 1)
    }

    pub fn last_child(&self) -> Option<NodeId> {
        self.children().last().cloned()
    }

    pub fn nth_child(&self, pos: usize) -> Option<NodeId> {
        self.children().iter().nth(pos).cloned()
    }

    fn insert_child(&self, child: NodeId, pos: usize) {
        use ValueOrContainer::Container;
        let Some(Container(container)) = self.0.get("children") else { return };
        let Ok(list) = container.into_list() else { return };
        let _ = list.insert(pos, child);
    }

    fn append_child(&self, child: NodeId) {
        let pos = self.children().len();
        self.insert_child(child, pos);
    }

    pub fn children(&self) -> Vec<NodeId> {
        use ValueOrContainer::Container;
        let Some(Container(container)) = self.0.get("children") else { return vec![] };
        let Ok(list) = container.into_list() else { return vec![] };

        list
            .to_vec()
            .into_iter()
            .map(NodeId::try_from)
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default()
    }
}

impl From<NodeId> for LoroValue {
    fn from(value: NodeId) -> Self {
        LoroValue::Binary(value.as_bytes().into())
    }
}

impl TryFrom<LoroValue> for NodeId {
    type Error = anyhow::Error;

    fn try_from(value: LoroValue) -> Result<Self, Self::Error> {
        if let LoroValue::Binary(bytes) = value {
            let node_id = NodeId::try_from(&bytes)?;
            Ok(node_id)
        } else {
            bail!("expecting LoroValue to be binary for NodeId")
        }
    }
}

struct Arena(LoroMap);

impl Arena {
    fn insert(&self, node: Node) {
        let key = node.id().to_string();
        self.0.insert(&key, node);
    }

    fn delete(&self, id: &NodeId) {
        let key = id.to_string();
        let _ = self.0.delete(&key);
    }

    fn get(&self, id: &NodeId) -> Option<Node> {
        let key = id.to_string();
        self.0.get(&key)?
            .into_container().ok()?
            .into_map()
            .ok()
            .map(Node)
    }
}