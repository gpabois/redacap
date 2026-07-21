use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use crate::{
    job::{Job, JobId, JobState},
    model::declaration::{ModelDeclaration, ModelId},
    network::worker::info::WorkerInfo,
    tools::{catalog::ToolId, declaration::ToolDeclaration},
};

/// Métadonnées réseau attachées à un membre du cluster Raft — c'est ce
/// qu'openraft appelle "Node" (à ne pas confondre avec un worker du système).
#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftNode {
    pub peer_id: Option<PeerId>,
    /// Multiaddr libp2p, ex: "/ip4/10.0.0.2/tcp/4001"
    pub addr: String,
}

pub type RaftNodeId = u64;

// ---------------------------------------------------------------------------
// Commandes répliquées via le log Raft
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ControlPlaneRequest {
    SubmitJob(Job),
    AssignJob { job_id: JobId, worker: PeerId },
    CommitState { job_id: JobId, new_state: JobState },
    RegisterWorker { worker: WorkerInfo },
    /// Un pair `Persistency` (voir `network::persistency`) s'est fait
    /// connaître — ajouté aux détenteurs de secours pour toute session (voir
    /// `ControlPlaneState::session_holders` et `network::cp::reconcile`).
    RegisterPersistency { peer_id: PeerId },
    /// Crée ou remplace la déclaration d'un modèle du catalogue (voir
    /// `RpcCall::SET_MODEL`). Persisté localement au repos par chaque nœud
    /// control plane qui applique cette entrée (voir
    /// `ControlPlaneStateMachineStore::persist_model_mutation`), pas
    /// seulement par le leader qui l'a proposée.
    SetModel { id: ModelId, declaration: ModelDeclaration },
    /// Retire un modèle du catalogue (voir `RpcCall::REMOVE_MODEL`).
    RemoveModel { id: ModelId },
    /// Crée ou remplace la déclaration d'un tool du catalogue (voir
    /// `RpcCall::SET_TOOL`). Persisté localement au repos par chaque nœud
    /// control plane qui applique cette entrée (voir
    /// `ControlPlaneStateMachineStore::persist_tool_mutation`), pas
    /// seulement par le leader qui l'a proposée. Ne dit rien de qui exécute
    /// ce tool (voir `RpcCall::REGISTER_RPC` et
    /// `tools::client::ToolClient::register_executor`, non répliqués).
    SetTool { id: ToolId, declaration: ToolDeclaration },
    /// Retire un tool du catalogue (voir `RpcCall::REMOVE_TOOL`).
    RemoveTool { id: ToolId },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ControlPlaneResponse {
    Ok,
    Rejected { reason: String },
}

// Déclaration du TypeConfig openraft
// ---------------------------------------------------------------------------
//
// Ce macro génère un type `TypeConfig` (unit struct) qui implémente
// `openraft::RaftTypeConfig` avec les associated types suivants. C'est ce
// type qui est passé en paramètre générique partout ailleurs (Raft<TypeConfig>,
// RaftLogStorage<TypeConfig>, etc.)

openraft::declare_raft_types!(
    pub TypeConfig:
        D = ControlPlaneRequest,
        R = ControlPlaneResponse,
        NodeId = RaftNodeId,
        Node = RaftNode,
);
