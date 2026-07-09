use std::collections::HashMap;
use std::error::Error as StdError;

use futures::StreamExt;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm, identity, mdns, noise, tcp, yamux};
use openraft::error::{InstallSnapshotError, NetworkError, RaftError, RPCError, RemoteError, Unreachable};
use openraft::network::{RPCOption, RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse, VoteRequest,
    VoteResponse
};
use openraft::Raft;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::model::catalog::types::{NodeId, RaftNode, TypeConfig};

const RAFT_PROTOCOL: &str = "/marie/raft/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum RaftRpcRequest {
    AppendEntries(AppendEntriesRequest<TypeConfig>),
    Vote(VoteRequest<NodeId>),
    InstallSnapshot(InstallSnapshotRequest<TypeConfig>)
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) enum RaftRpcResponse {
    AppendEntries(Result<AppendEntriesResponse<NodeId>, RaftError<NodeId>>),
    Vote(Result<VoteResponse<NodeId>, RaftError<NodeId>>),
    InstallSnapshot(Result<InstallSnapshotResponse<NodeId>, RaftError<NodeId, InstallSnapshotError>>)
}

#[derive(Error, Debug)]
#[error("réponse inattendue reçue du nœud raft distant")]
struct UnexpectedResponse;

#[derive(Error, Debug, Clone)]
pub enum DriverError {
    #[error("adresse de nœud invalide : {0}")]
    InvalidAddress(String),
    #[error("le nœud raft n'est pas joignable : {0}")]
    Unreachable(String),
    #[error("le pilote réseau libp2p s'est arrêté")]
    DriverStopped
}

#[derive(NetworkBehaviour)]
pub(super) struct MarieBehaviour {
    raft: request_response::json::Behaviour<RaftRpcRequest, RaftRpcResponse>,
    mdns: mdns::tokio::Behaviour
}

enum Command {
    Send {
        peer: PeerId,
        addr: Multiaddr,
        request: RaftRpcRequest,
        respond_to: oneshot::Sender<Result<RaftRpcResponse, DriverError>>
    }
}

/// Poignée permettant de piloter le nœud libp2p depuis l'extérieur de la tâche qui l'exécute.
#[derive(Clone)]
pub struct SwarmHandle {
    commands: mpsc::UnboundedSender<Command>
}

impl SwarmHandle {
    async fn send(&self, peer: PeerId, addr: Multiaddr, request: RaftRpcRequest) -> Result<RaftRpcResponse, DriverError> {
        let (respond_to, rx) = oneshot::channel();

        self.commands
            .send(Command::Send { peer, addr, request, respond_to })
            .map_err(|_| DriverError::DriverStopped)?;

        rx.await.map_err(|_| DriverError::DriverStopped)?
    }
}

/// Construit le nœud libp2p (transport TCP+Noise+Yamux, découverte mDNS et protocole raft),
/// puis lance la boucle d'évènements dans une tâche dédiée.
///
/// Retourne une poignée pour émettre des requêtes raft, l'adresse complète
/// (`/.../p2p/<peer_id>`) sur laquelle ce nœud écoute, ainsi qu'un canal à usage unique pour
/// fournir l'instance [`Raft`] à la tâche réseau une fois construite : `Raft::new` a en effet
/// besoin de [`SwarmHandle`] (via [`MarieNetworkFactory`]) pour émettre des RPC, alors que la
/// tâche réseau a besoin de l'instance `Raft` pour répondre aux RPC entrantes. Les évènements
/// entrants sont mis en attente le temps que ce canal soit alimenté.
pub(super) async fn spawn(
    keypair: identity::Keypair,
    listen_addr: Multiaddr
) -> Result<(SwarmHandle, Multiaddr, oneshot::Sender<Raft<TypeConfig>>), Box<dyn StdError + Send + Sync>> {
    let local_peer_id = keypair.public().to_peer_id();

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_behaviour(|_| {
            let raft_behaviour = request_response::json::Behaviour::<RaftRpcRequest, RaftRpcResponse>::new(
                [(StreamProtocol::new(RAFT_PROTOCOL), ProtocolSupport::Full)],
                request_response::Config::default()
            );
            let mdns_behaviour = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)
                .map_err(|err| Box::new(err) as Box<dyn StdError + Send + Sync>)?;

            Ok::<_, Box<dyn StdError + Send + Sync>>(MarieBehaviour { raft: raft_behaviour, mdns: mdns_behaviour })
        })?
        .build();

    swarm.listen_on(listen_addr)?;

    let advertised_addr = loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                break match address.with_p2p(local_peer_id) {
                    Ok(addr) => addr,
                    Err(addr) => addr
                };
            }
            SwarmEvent::ListenerError { error, .. } => return Err(Box::new(error) as Box<dyn StdError + Send + Sync>),
            _ => continue
        }
    };

    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let (raft_tx, raft_rx) = oneshot::channel();

    tokio::spawn(SwarmDriver { swarm, commands: command_rx, pending: HashMap::new() }.run(raft_rx));

    Ok((SwarmHandle { commands: command_tx }, advertised_addr, raft_tx))
}

struct SwarmDriver {
    swarm: Swarm<MarieBehaviour>,
    commands: mpsc::UnboundedReceiver<Command>,
    pending: HashMap<OutboundRequestId, oneshot::Sender<Result<RaftRpcResponse, DriverError>>>
}

impl SwarmDriver {
    async fn run(mut self, raft_rx: oneshot::Receiver<Raft<TypeConfig>>) {
        let Ok(raft) = raft_rx.await else { return };

        loop {
            tokio::select! {
                command = self.commands.recv() => match command {
                    Some(command) => self.handle_command(command),
                    None => return,
                },
                event = self.swarm.select_next_some() => self.handle_event(event, &raft).await,
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Send { peer, addr, request, respond_to } => {
                let request_id = self.swarm.behaviour_mut().raft.send_request_with_addresses(&peer, request, vec![addr]);
                self.pending.insert(request_id, respond_to);
            }
        }
    }

    async fn handle_event(&mut self, event: SwarmEvent<MarieBehaviourEvent>, raft: &Raft<TypeConfig>) {
        match event {
            SwarmEvent::Behaviour(MarieBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
                for (peer_id, addr) in peers {
                    self.swarm.add_peer_address(peer_id, addr);
                }
            }
            SwarmEvent::Behaviour(MarieBehaviourEvent::Raft(request_response::Event::Message { message, .. })) => {
                self.handle_message(message, raft).await;
            }
            SwarmEvent::Behaviour(MarieBehaviourEvent::Raft(request_response::Event::OutboundFailure {
                request_id,
                error,
                ..
            })) => {
                if let Some(respond_to) = self.pending.remove(&request_id) {
                    let _ = respond_to.send(Err(DriverError::Unreachable(error.to_string())));
                }
            }
            _ => {}
        }
    }

    async fn handle_message(&mut self, message: request_response::Message<RaftRpcRequest, RaftRpcResponse>, raft: &Raft<TypeConfig>) {
        match message {
            request_response::Message::Request { request, channel, .. } => {
                let response = dispatch(request, raft).await;
                let _ = self.swarm.behaviour_mut().raft.send_response(channel, response);
            }
            request_response::Message::Response { request_id, response } => {
                if let Some(respond_to) = self.pending.remove(&request_id) {
                    let _ = respond_to.send(Ok(response));
                }
            }
        }
    }
}

async fn dispatch(request: RaftRpcRequest, raft: &Raft<TypeConfig>) -> RaftRpcResponse {
    match request {
        RaftRpcRequest::AppendEntries(rpc) => RaftRpcResponse::AppendEntries(raft.append_entries(rpc).await),
        RaftRpcRequest::Vote(rpc) => RaftRpcResponse::Vote(raft.vote(rpc).await),
        RaftRpcRequest::InstallSnapshot(rpc) => RaftRpcResponse::InstallSnapshot(raft.install_snapshot(rpc).await)
    }
}

/// Extrait le `PeerId` et l'adresse de connexion depuis l'adresse enregistrée pour un nœud
/// raft (`RaftNode::addr`), qui doit se terminer par `/p2p/<peer_id>`.
fn parse_node_addr(addr: &str) -> Result<(PeerId, Multiaddr), DriverError> {
    let multiaddr: Multiaddr = addr.parse().map_err(|_| DriverError::InvalidAddress(addr.to_owned()))?;

    let peer_id = multiaddr
        .iter()
        .find_map(|protocol| match protocol {
            Protocol::P2p(peer_id) => Some(peer_id),
            _ => None
        })
        .ok_or_else(|| DriverError::InvalidAddress(addr.to_owned()))?;

    Ok((peer_id, multiaddr))
}

#[derive(Clone)]
pub(super) struct MarieNetworkFactory {
    handle: SwarmHandle
}

impl MarieNetworkFactory {
    pub(super) fn new(handle: SwarmHandle) -> Self {
        Self { handle }
    }
}

impl RaftNetworkFactory<TypeConfig> for MarieNetworkFactory {
    type Network = MarieNetworkClient;

    async fn new_client(&mut self, target: NodeId, node: &RaftNode) -> Self::Network {
        MarieNetworkClient { target, addr: node.addr.clone(), handle: self.handle.clone() }
    }
}

pub(super) struct MarieNetworkClient {
    target: NodeId,
    addr: String,
    handle: SwarmHandle
}

impl MarieNetworkClient {
    async fn send(&self, request: RaftRpcRequest) -> Result<RaftRpcResponse, DriverError> {
        let (peer, addr) = parse_node_addr(&self.addr)?;
        self.handle.send(peer, addr, request).await
    }
}

impl RaftNetwork<TypeConfig> for MarieNetworkClient {
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<TypeConfig>,
        _option: RPCOption
    ) -> Result<AppendEntriesResponse<NodeId>, RPCError<NodeId, RaftNode, RaftError<NodeId>>> {
        match self.send(RaftRpcRequest::AppendEntries(rpc)).await.map_err(|err| RPCError::Unreachable(Unreachable::new(&err)))? {
            RaftRpcResponse::AppendEntries(result) => result.map_err(|err| RPCError::RemoteError(RemoteError::new(self.target, err))),
            _ => Err(RPCError::Network(NetworkError::new(&UnexpectedResponse)))
        }
    }

    async fn install_snapshot(
        &mut self,
        rpc: InstallSnapshotRequest<TypeConfig>,
        _option: RPCOption
    ) -> Result<InstallSnapshotResponse<NodeId>, RPCError<NodeId, RaftNode, RaftError<NodeId, InstallSnapshotError>>> {
        match self.send(RaftRpcRequest::InstallSnapshot(rpc)).await.map_err(|err| RPCError::Unreachable(Unreachable::new(&err)))? {
            RaftRpcResponse::InstallSnapshot(result) => result.map_err(|err| RPCError::RemoteError(RemoteError::new(self.target, err))),
            _ => Err(RPCError::Network(NetworkError::new(&UnexpectedResponse)))
        }
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<NodeId>,
        _option: RPCOption
    ) -> Result<VoteResponse<NodeId>, RPCError<NodeId, RaftNode, RaftError<NodeId>>> {
        match self.send(RaftRpcRequest::Vote(rpc)).await.map_err(|err| RPCError::Unreachable(Unreachable::new(&err)))? {
            RaftRpcResponse::Vote(result) => result.map_err(|err| RPCError::RemoteError(RemoteError::new(self.target, err))),
            _ => Err(RPCError::Network(NetworkError::new(&UnexpectedResponse)))
        }
    }
}
