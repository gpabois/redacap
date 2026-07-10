use std::collections::HashMap;

use futures::StreamExt as _;
use libp2p::{gossipsub::{self, IdentTopic}, identify, request_response::{self, Message::{Request, Response}}, swarm::SwarmEvent};
use tokio::{select, sync::mpsc};
use tracing::{info, error};

use crate::{job::{self, Job, JobKind, JobStealResponse, WorkerEvent}, network::{MarieBehaviour, MarieBehaviourEvent, MarieSwarm, peer::NodeKind, start_swarm}};


/// Orchestrator de tâches
pub async fn start_orchestrator() -> Result<(), anyhow::Error> {
    use SwarmEvent::Behaviour;
    use MarieBehaviourEvent::{JobNegociation, Identify};
    use request_response::Event::Message; 
    use request_response::Message::Request;
    use identify::Event::Received;
    use NodeKind::WorkerOrchestrator;

    let mut state: job::OrchestratorState = Default::default();

    let mut swarm = start_swarm(WorkerOrchestrator, |swarm| {

    }).await?;

    loop {
        select! {
            event = swarm.select_next_some() => {
                match event {
                    Behaviour(Identify(Received{peer_id, info, ..})) => {
                        if info.agent_version.contains("worker") {
                            info!("Worker détecté {peer_id}")
                        }
                    },
                    Behaviour(JobNegociation(Message{peer, message: Request{request, channel, ..}, ..})) 
                    => {
                        let resp = if !state.is_claimed(&request.job) {
                            info!("🥇 Le worker {} a volé le job {} avec succès !", peer, request.job.id);
                            state.has_been_claimed(request.job.clone(), peer);
                            job::JobStealResponse::CanSteal(request.job)
                        } else {
                            info!("🛑 Le worker {} a tenté de voler le job {}, mais trop tard.", peer, request.job.id);
                            job::JobStealResponse::AlreadyStolen
                        };

                        let _ = swarm.behaviour_mut().job_negociation.send_response(channel, resp)
                        .inspect_err(|_| {
                            error!("Echec de la transmission de la négociation au worker")
                        });
                    },
                    _ => {}
                }
            }
        }
    }
}

/// Travailleur de tâches
pub async fn start_worker(
    mut swarm: libp2p::Swarm<MarieBehaviour>,
    mut state: job::WorkerState,
) -> Result<(), anyhow::Error> {
    use NodeKind::Worker;
    use SwarmEvent::Behaviour;
    use MarieBehaviourEvent::{WorkerGossip, JobNegociation};
    use gossipsub::Event::Message;

    let mut state = job::WorkerState::new();
    let mut swarm = start_swarm(Worker, |swarm| {
        let _ = swarm.behaviour_mut().worker_gossip.subscribe(&job_announcements_topic());
    }).await?;

    loop {
        match swarm.select_next_some().await {
            Behaviour(WorkerGossip(Message{message, ..})) 
                if let Ok(job) = serde_json::from_slice::<job::Job>(&message.data)
            => {
            },
 
            _ => {}
        }
    }
}

#[inline]
fn job_announcements_topic() -> IdentTopic{
    gossipsub::IdentTopic::new("job-announcements")
}

/// Spawn un job sur le réseau
pub fn spawn_job(swarm: &mut MarieSwarm, job: JobKind) -> Result<(), anyhow::Error> {
    let ann = Job {
        id: shared::id::generate_id(),
        job,
    };

    let raw = serde_json::to_vec(&ann)?;
    swarm.behaviour_mut().worker_gossip.publish(job_announcements_topic(), raw)?;
    Ok(())
}