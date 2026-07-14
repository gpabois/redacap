use std::collections::HashMap;

use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use crate::id::ID;
use tokio::sync::{mpsc, oneshot};

use crate::agent::GlobalAgentId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobKind {
    RunAgent(GlobalAgentId)
}

pub type JobId = ID;
// Diffusé sur Gossipsub par le Control Plane
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: ID,
    pub kind: JobKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum JobState {
    Pending,
    Scheduled { worker: PeerId },
    /// `worker` : rapporté par le worker lui-même (voir
    /// `network::worker::report_job_state`), pas recalculé par le control
    /// plane — nécessaire pour dériver les détenteurs actifs d'une session
    /// directement depuis `jobs` (voir `ControlPlaneState::session_holders`)
    /// sans pointeur séparé.
    Running { worker: PeerId },
    Completed { result: String },
    Failed { error: String, retry_count: u32 },
    Retrying,
}
