use std::collections::HashMap;

use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use shared::id::ID;
use tokio::sync::{mpsc, oneshot};

use crate::agent::GlobalAgentId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobKind {
    RunAgent(GlobalAgentId)
}

// Diffusé sur Gossipsub par le Control Plane
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: ID,
    pub job: JobKind,
}

// Envoyé par le Worker via Request-Response pour "bloquer" le job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStealRequest {
    pub job: Job,
}

// Réponse du Control Plane
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobStealResponse {
    CanSteal(Job),
    AlreadyStolen
}


pub struct WorkerState {
    pub pending_jobs: HashMap<ID, Job>,
    pub running_jobs: HashMap<ID, Job>,
    pub failed_jobs: HashMap<ID, Job>,
    pub event_rx: mpsc::UnboundedReceiver<WorkerEvent>,
    pub event_tx: mpsc::UnboundedSender<WorkerEvent>,
}

impl WorkerState {
    #[must_use]
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            pending_jobs: Default::default(),
            running_jobs: Default::default(),
            failed_jobs: Default::default(),
            event_tx,
            event_rx,
        }
    }
}


pub enum WorkerEvent {
    JobTerminated {
        job_id: ID,
    },
    JobFailed {
        job_id: ID,
        error: anyhow::Error
    }
}

#[derive(Debug, Clone)]
pub struct ClaimedJob {
    job: Job,
    by: PeerId
}

#[derive(Debug, Default, Clone)]
pub struct OrchestratorState {
    pub claimed_jobs: HashMap<ID, ClaimedJob>,
}

impl OrchestratorState {
    pub fn is_claimed(&self, job: &Job) -> bool {
        self.claimed_jobs.contains_key(&job.id)
    }

    pub fn has_been_claimed(&mut self, job: Job, by: PeerId) {
        self.claimed_jobs.insert(job.id, ClaimedJob { job, by });

    }
}