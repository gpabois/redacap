use std::{collections::HashMap, sync::{Arc, Mutex}};

use leptos::prelude::*;
use leptos::reactive::signal::RwSignal;
use loro::{ContainerID, ContainerTrait, LoroText, UpdateOptions};

use crate::{data::{NodeData, NodeKind}, editor::{components::EditOptions, tools::ToolGroup}, id::NodeId, model::{LegalActProject, Node}, selection::Cursor};

#[derive(Clone)]
pub struct TextSignal {
    text: LoroText,
    signal: RwSignal<u64>
}

impl TextSignal {
    pub fn get(&self) -> String {
        self.signal.get();
        self.text.to_string()
    }

    pub fn update(&self, text: impl ToString) {
        self.text
            .update(&text.to_string(), UpdateOptions::default())
            .unwrap();
    }

    /// Insère `text` à la position unicode `pos`.
    pub fn insert(&self, pos: usize, text: &str) {
        self.text.insert(pos, text).unwrap();
    }

    /// Supprime `len` caractères à partir de la position unicode `pos`.
    pub fn delete(&self, pos: usize, len: usize) {
        self.text.delete(pos, len).unwrap();
    }
}

pub struct NodeSignal {
    state: EditorState,
    id: NodeId,
    signal: RwSignal<u64>
}

impl NodeSignal {
    pub fn get(&self) -> Node {
        let _ = self.signal.get();
        self.state.act.try_node(&self.id).unwrap()
    }

    pub fn kind(&self) -> NodeKind {
        self.signal.get();
        self.state.act.kind(&self.id)
    }

    pub fn children(&self) -> Vec<NodeId> {
        self.signal.get();
        self.state.act.children_of(&self.id)
    }

    pub fn text(&self) -> TextSignal {
        let text = self.state.act.text(&self.id);
        let signal = self.state.container_signal(text.to_container().id());
        TextSignal {
            text,
            signal
        }
    }
}

#[derive(Clone)]
pub struct EditorState {
    act: LegalActProject,
    orders: Arc<Mutex<HashMap<NodeId, usize>>>,
    toolbar: RwSignal<HashMap<String, ToolGroup>>,
    pub cursor: RwSignal<Option<Cursor>>,
    pub selection: RwSignal<Option<crate::selection::Span>>,
    signals: Arc<Mutex<HashMap<NodeId, RwSignal<u64>>>>,
    containers_signals: Arc<Mutex<HashMap<ContainerID, RwSignal<u64>>>>,
}

impl EditorState {
    pub fn new(act: LegalActProject) -> Self {
        let signals = Arc::new(Mutex::new(HashMap::<NodeId, RwSignal<u64>>::new()));
        let containers_signals = Arc::new(Mutex::new(HashMap::<ContainerID, RwSignal<u64>>::new()));

        let cont_sigs = containers_signals.clone();
        let toolbar = RwSignal::new(HashMap::default());

        let _ = act.subscribe_root(Arc::new(move |event| {
            let map = cont_sigs.lock().unwrap();

            // On ne réveille QUE les composants abonnés aux conteneurs modifiés
            for container_diff in &event.events {
                let id = &container_diff.target;
                if let Some(signal) = map.get(id) {
                    signal.update(|v| *v += 1);
                }
            }
        }));

        Self {
            act, 
            orders: Arc::new(Mutex::new(HashMap::default())),
            toolbar,
            containers_signals,
            signals,
            selection: RwSignal::new(None),
            cursor: RwSignal::new(None)
        }
        
    }

    pub fn set_toolgroup(&self, id: impl ToString, group: ToolGroup) {
        self.toolbar.update(|toolbar| {
            toolbar.insert(id.to_string(), group);
        });
    }

    pub fn unset_toolgroup(&self, id: impl ToString) {
        self.toolbar.update(|toolbar| {
            toolbar.remove(&id.to_string());
        });
    }

    pub fn cursor(&self) -> Option<Cursor> {
        self.cursor.get().clone()
    }

    /// Insère `text` dans le nœud `Plain` pointé par le curseur courant, à
    /// la position du curseur, puis avance le curseur de la longueur (en
    /// caractères) du texte inséré.
    pub fn insert_at_cursor(&self, text: &str) {
        let Some(cursor) = self.cursor.get_untracked() else { return };
        let Some(node) = self.try_node(&cursor.id) else { return };

        node.text().insert(cursor.pos, text);

        let pos = cursor.pos + text.chars().count();
        self.cursor.set(Some(Cursor { id: cursor.id, pos }));
    }

    /// Supprime le caractère précédant le curseur (touche Retour arrière) et
    /// recule le curseur d'une position. Sans effet en début de nœud.
    pub fn delete_backward(&self) {
        let Some(cursor) = self.cursor.get_untracked() else { return };
        if cursor.pos == 0 { return }
        let Some(node) = self.try_node(&cursor.id) else { return };

        node.text().delete(cursor.pos - 1, 1);

        let pos = cursor.pos - 1;
        self.cursor.set(Some(Cursor { id: cursor.id, pos }));
    }

    /// Supprime le caractère suivant le curseur (touche Suppr) sans déplacer
    /// le curseur. Sans effet en fin de nœud.
    pub fn delete_forward(&self) {
        let Some(cursor) = self.cursor.get_untracked() else { return };
        let Some(node) = self.try_node(&cursor.id) else { return };

        let char_len = node.text().get().chars().count();
        if cursor.pos >= char_len { return }

        node.text().delete(cursor.pos, 1);

        // La position ne change pas, mais on ré-émet le curseur pour forcer
        // le nœud `Plain` concerné à se re-rendre : le signal du conteneur
        // texte (bump via `subscribe_root`) ne suffit pas à lui seul à
        // déclencher ce re-rendu de manière fiable ici.
        self.cursor.set(Some(Cursor { id: cursor.id, pos: cursor.pos }));
    }

    /// Avance le curseur d'une position (flèche droite). En fin de nœud,
    /// saute au début du prochain nœud `Plain` du document.
    pub fn forward_cursor(&self) {
        let Some(cursor) = self.cursor.get_untracked() else { return };
        let Some(node) = self.try_node(&cursor.id) else { return };

        let char_len = node.text().get().chars().count();
        if cursor.pos < char_len {
            self.cursor.set(Some(Cursor { id: cursor.id, pos: cursor.pos + 1 }));
            return;
        }

        if let Some(id) = self.next_plain_leaf(&cursor.id) {
            self.cursor.set(Some(Cursor { id, pos: 0 }));
        }
    }

    /// Recule le curseur d'une position (flèche gauche). En début de nœud,
    /// saute à la fin du nœud `Plain` précédent du document.
    pub fn backward_cursor(&self) {
        let Some(cursor) = self.cursor.get_untracked() else { return };

        if cursor.pos > 0 {
            self.cursor.set(Some(Cursor { id: cursor.id, pos: cursor.pos - 1 }));
            return;
        }

        let Some(id) = self.prev_plain_leaf(&cursor.id) else { return };
        let Some(node) = self.try_node(&id) else { return };
        let pos = node.text().get().chars().count();
        self.cursor.set(Some(Cursor { id, pos }));
    }

    /// Prochaine feuille `Plain` après `id` dans l'ordre du document, en
    /// sautant les feuilles d'un autre type (ex : un nœud conteneur encore
    /// vide).
    fn next_plain_leaf(&self, id: &NodeId) -> Option<NodeId> {
        let mut next = self.act.next_leaf(id);
        while let Some(candidate) = next {
            if matches!(self.act.kind(&candidate), NodeKind::Plain) {
                return Some(candidate);
            }
            next = self.act.next_leaf(&candidate);
        }
        None
    }

    /// Feuille `Plain` précédant `id` dans l'ordre du document, en sautant
    /// les feuilles d'un autre type.
    fn prev_plain_leaf(&self, id: &NodeId) -> Option<NodeId> {
        let mut prev = self.act.prev_leaf(id);
        while let Some(candidate) = prev {
            if matches!(self.act.kind(&candidate), NodeKind::Plain) {
                return Some(candidate);
            }
            prev = self.act.prev_leaf(&candidate);
        }
        None
    }

    pub fn toolbar(&self) -> Vec<(String, ToolGroup)> {
        self.toolbar
            .get()
            .iter()
            .map(|(id, group)| (id.clone(), group.clone()))
            .collect()
    }

    pub fn title(&self) -> TextSignal {
        let text = self.act.title();
        let signal = self.containers_signals
            .lock()
            .unwrap()
            .entry(text.to_container().id()).or_insert_with(|| RwSignal::new(0))
            .to_owned();
        
        TextSignal {
            text,
            signal
        }
    }

    pub fn compute_leaf_orders(&self) {
        let orders = self.act.leafs(&self.act.body()).enumerate().map(|(order, id)| (id, order));
        self.orders.lock().unwrap().extend(orders);
    }

    pub fn visas(&self) -> NodeSignal {
        self.try_node(&self.act.visas()).unwrap()
    }

    pub fn considerants(&self) -> NodeSignal {
        self.try_node(&self.act.considerants()).unwrap()
    }

    pub fn sur(&self) -> NodeSignal {
        self.try_node(&self.act.sur()).unwrap()
    }

    pub fn body(&self) -> NodeSignal {
        self.try_node(&self.act.body()).unwrap()
    }

    pub fn act(&self) -> LegalActProject {
        self.act.clone()
    }

    pub fn kind(&self, id: &NodeId) -> NodeKind {
        self.try_node(id).unwrap().get().data().kind()
    }

    pub fn insert(&self, parent: &NodeId, data: impl Into<NodeData>, position: usize) {
        let child = self.act.create_node(data);
        self.act.insert_child(parent, &child, position);
        self.signal_node(parent);
    }

    pub fn delete(&self, node: &NodeId) {
        let parent = self.act.parent_of(node);
        self.act.delete(node);

        if let Some(parent) = parent {
            self.signal_node(&parent);
        }
    }

    pub fn r#move(&self, node: &NodeId, to: &NodeId, position: usize) {
        let parent = self.act.parent_of(node);
        self.act.r#move(node, to, position);

        if let Some(parent) = parent {
            self.signal_node(&parent);
        }

        self.signal_node(to);
    }

    pub fn try_node(&self, id: &NodeId) -> Option<NodeSignal> {
        let container_id = self.act.try_node(id)?.container_id();
        let signal = *self.signals.lock().unwrap()
            .entry(id.clone())
            .or_insert_with(|| RwSignal::new(0));
        self.containers_signals.lock().unwrap().insert(container_id, signal);

        Some(NodeSignal { state: self.clone(), id: id.clone(), signal })
    }

    pub fn node(&self, id: &NodeId) -> NodeSignal {
        self.try_node(id).unwrap()
    }

    fn signal_node(&self, id: &NodeId) {
        let signal = *self.signals.lock().unwrap()
            .entry(id.clone())
            .or_insert_with(|| {
                let signal = RwSignal::new(0);
                let container_id = self.act.node(id).container_id();
                self.register_container_signal(container_id, signal.clone());
                signal
            });

        
        signal.update(|epoch| *epoch += 1);
    }

    fn container_signal(&self, container_id: ContainerID) -> RwSignal<u64> {
        self.containers_signals
            .lock()
            .unwrap()
            .entry(container_id)
            .or_insert_with(|| RwSignal::new(0))  
            .to_owned()
    }

    fn register_container_signal(&self, container_id: ContainerID, signal: RwSignal<u64>) {
        self.containers_signals
            .lock()
            .unwrap()
            .insert(container_id, signal);
    }
}


#[derive(Clone)]
pub struct ContentEditorState {
    state: EditorState,
    root: NodeId,
    options: EditOptions
}