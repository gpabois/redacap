use std::sync::{Arc, Mutex};

use leptos::reactive::signal::RwSignal;
use loro::ContainerID;

use crate::{id::NodeId, model::LegalActProject};

pub struct NodeSignal {
    act: LegalActProject,
    id: NodeId,
    signal: RwSignal<u64>
}

impl NodeSignal {
    pub fn get(&self) -> Node {
        let _ = self.signal().get();
        self.act.node(&self.id).unwrap()
    }

    pub fn children(&self) -> Vec<NodeId> {
        self.act.children_of(self.get().id())
    }
}

#[derive(Clone)]
pub struct EditorState {
    act: LegalActProject,
    signals: Arc<Mutex<HashMap<ContainerID, RwSignal<u64>>>>,
}

impl EditorState {
    pub fn new(act: LegalActProject) -> Self {
        let signals = Arc::new(Mutex::new(HashMap::<ContainerID, RwSignal<u64>>::new()));

        let sigs = signals.clone();
        
        act.subscribe_root(Arc::new(move |event| {
            let map = sigs.lock().unwrap();
            
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
            signals
        }
        
    }

    pub fn visas(&self) -> NodeSignal {
        self.node(&self.act.visas()).unwrap()
    }

    pub fn node(&self, id: &NodeId) -> Option<NodeSignal> {
        let container_id = act.node(id)?.container_id();
        let mut map = self.signals.lock().unwrap();
        let signal = *map.entry(container_id.clone()).or_insert_with(|| RwSignal::new(0));
        Some(NodeSignal { act: self.act.clone(), id: id.clone(), signal })
    }
}