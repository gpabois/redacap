use std::fmt::Display;

use libp2p::PeerId;

pub struct PeerNode {
    pub peer_id: Option<PeerId>,
    pub node_kind: NodeKind
}

#[derive(strum_macros::Display, strum_macros::EnumString)]
pub enum NodeKind {
    ControlPlane,
    WorkerOrchestrator,
    Worker
}
