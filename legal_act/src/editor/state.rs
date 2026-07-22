use std::{collections::HashMap, sync::{Arc, Mutex}};

use leptos::prelude::*;
use leptos::reactive::signal::RwSignal;
use loro::ContainerID;

use crate::{data::{NodeData, NodeKind}, id::NodeId, model::{LegalActProject, Node}, selection::Cursor};

pub struct NodeSignal {
    act: LegalActProject,
    id: NodeId,
    signal: RwSignal<u64>
}

impl NodeSignal {
    pub fn get(&self) -> Node {
        let _ = self.signal.get();
        self.act.node(&self.id).unwrap()
    }

    pub fn kind(&self) -> NodeKind {
        self.signal.get();
        self.act.kind(&self.id)
    }

    pub fn children(&self) -> Vec<NodeId> {
        self.signal.get();
        self.act.children_of(&self.id)
    }

    pub fn text(&self) -> String {
        self.signal.get();
        self.act.text(&self.id)
    }
}

#[derive(Clone)]
pub struct EditorState {
    act: LegalActProject,
    orders: Arc<Mutex<HashMap<NodeId, usize>>>,
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

        act.subscribe_root(Arc::new(move |event| {
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
            containers_signals,
            signals,
            selection: RwSignal::new(None),
            cursor: RwSignal::new(None)
        }
        
    }

    pub fn compute_leaf_orders(&self) {
        let orders = self.act.leafs(&self.act.body()).enumerate().map(|(order, id)| (id, order));
        self.orders.lock().unwrap().extend(orders);
    }

    pub fn visas(&self) -> NodeSignal {
        self.try_node(&self.act.visas()).unwrap()
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
        self.signal(parent);
    }

    pub fn delete(&self, node: &NodeId) {
        let parent = self.act.parent_of(node);
        self.act.delete(node);

        if let Some(parent) = parent {
            self.signal(&parent);
        }
    }

    pub fn r#move(&self, node: &NodeId, to: &NodeId, position: usize) {
        let parent = self.act.parent_of(node);
        self.act.r#move(node, to, position);

        if let Some(parent) = parent {
            self.signal(&parent);
        }

        self.signal(to);
    }

    pub fn try_node(&self, id: &NodeId) -> Option<NodeSignal> {
        let container_id = self.act.node(id)?.container_id();
        let signal = *self.signals.lock().unwrap()
            .entry(id.clone())
            .or_insert_with(|| RwSignal::new(0));
        self.containers_signals.lock().unwrap().insert(container_id, signal);

        Some(NodeSignal { act: self.act.clone(), id: id.clone(), signal })
    }

    pub fn node(&self, id: &NodeId) -> NodeSignal {
        self.try_node(id).unwrap()
    }

    fn signal(&self, id: &NodeId) {
        let signal = *self.signals.lock().unwrap()
            .entry(id.clone())
            .or_insert_with(|| RwSignal::new(0));
        signal.update(|epoch| *epoch += 1);
    }
}