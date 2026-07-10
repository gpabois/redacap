use std::collections::HashMap;
use futures::StreamExt as _;
use libp2p::{PeerId, identify, request_response::{self, OutboundRequestId}, swarm::SwarmEvent};
use tokio::{select, sync::{mpsc, oneshot}};
use tracing::{warn, info};

use crate::network::{MarieSwarm, cp};

pub enum NetworkCommand {
    ControlPlaneRemoteProcedureCall {
        tx: oneshot::Sender<cp::rpc::RpcResult>,
        call: cp::rpc::RpcCall
    }
}

pub struct NetworkActor {
    swarm: MarieSwarm,
    commands_rx: mpsc::UnboundedReceiver<NetworkCommand>,
    commands_tx: mpsc::UnboundedSender<NetworkCommand>,
    // Control plane's remote procedure call -----------
    cp_peer_id: Option<PeerId>,
    cp_rpc_tx: mpsc::Sender<cp::rpc::RpcCall>,
    cp_rpc_rx: mpsc::Receiver<cp::rpc::RpcCall>,
    pending_cp_rpc: HashMap<OutboundRequestId, oneshot::Sender<cp::rpc::RpcResult>>,
    
}

impl NetworkActor {
    pub async fn control_plane_call(&mut self, call: cp::rpc::RpcCall) -> Result<cp::rpc::RpcResult, anyhow::Error> {
        use NetworkCommand::ControlPlaneRemoteProcedureCall;

        let (tx, rx) = oneshot::channel();
        self.commands_tx.send(ControlPlaneRemoteProcedureCall{tx, call});
        let ret = rx.await?;
        Ok(ret)
    }

    pub async fn run(mut self) {
        use NetworkCommand::*;
        use SwarmEvent::Behaviour;
        use request_response::Event as ReqResEvent;
        use identify::Event as IdEvent;
        use super::MarieBehaviourEvent::{CpRpc, Identify};

        loop {
            select! {
                Some(ControlPlaneRemoteProcedureCall{tx, call}) = self.commands_rx.recv() => {
                    let Some(peer_id) = self.cp_peer_id else {
                        warn!("Aucun control plane n'est connu de ce noeud...");
                        continue;
                    };

                    let id = self.swarm.behaviour_mut().cp_rpc.send_request(&peer_id, call);
                    self.pending_cp_rpc.insert(id, tx);
                },
                event = self.swarm.select_next_some() => {
                    match event {
                        Behaviour(CpRpc(ReqResEvent::Message{message, peer, ..}))
                        if let request_response::Message::Request{request, ..} = message
                         => {
                            
                        },
                        Behaviour(Identify(IdEvent::Received { peer_id, info, .. })) => {
                            if info.agent_version.contains("ControlPlane") {
                                info!("Control plane trouvé ! PeerId: {peer_id}");
                                self.cp_peer_id = Some(peer_id);
                            }
                        },
                        _ => {}
                    }
                }

            }
        }
    
    }
}