pub mod rpc;

use std::{collections::HashMap, time::{Instant, Duration}};

use futures::{StreamExt as _, channel::oneshot};
use libp2p::{PeerId, Swarm, request_response};
use serde::{Deserialize, Serialize};
use shared::id::ID;
use tokio::time::interval;

use crate::{model::{catalog::ModelCatalog, declaration::ModelDeclaration}, network::{MarieSwarm, peer::NodeKind, start_swarm}};

pub struct NodeHealth {
    pub last_seen: Instant,
    pub rtt: Option<Duration>, // Round-Trip Time (latence)
    pub status: NodeStatus,
}

pub enum NodeStatus {
    Alive,
    Dead
}

#[derive(Default)]
pub struct ControlPlaneState {
    pub nodes: HashMap<PeerId, NodeHealth>,
    pub model: ModelCatalog,
}

pub async fn start_control_plane() -> Result<(), anyhow::Error> {
    use NodeKind::ControlPlane;

    let mut state  = ControlPlaneState::default();
    let mut health_check_timer = interval(Duration::from_secs(4));

    let mut swarm = start_swarm(ControlPlane, |_| {}).await?;
    
    loop {
        tokio::select! {
            _ = health_check_timer.tick() => {
                let now = Instant::now();

                let dead: Vec<PeerId> = state.nodes.iter().filter(|(peer_id, health)| {
                    now.duration_since(health.last_seen) > Duration::from_secs(10)
                })
                .map(|(peer_id, _)| *peer_id)
                .collect();

                state.nodes.retain(|peer_id, _| !dead.contains(peer_id));

                for peer_id in dead.into_iter() {
                    let _ = broadcast_node_death(peer_id, &mut swarm);
                }

            },
            event = swarm.select_next_some() => {
                
            }
        }
    }
}

fn broadcast_node_death(peer_id: PeerId, swarm: &mut MarieSwarm) {

}
