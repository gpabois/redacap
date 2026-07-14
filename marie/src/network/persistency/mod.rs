use std::sync::Arc;

use anyhow::bail;
use futures::StreamExt as _;
use libp2p::PeerId;
use tokio::sync::oneshot;
use tracing::warn;
use yrs::{StateVector, updates::{decoder::Decode, encoder::Encode}};

use crate::{
    network::{
        actor::{NetworkActor, NetworkClient, NetworkEvent},
        cp::rpc::{RpcCall, RpcResult, SessionFetchRequest},
        peer::NodeKind,
        start_swarm,
    },
    persistency::{SessionFilesystem, SessionStore},
    secret::SecretManager,
    session::{SessionId, crdt::YrsSession, sync::{SESSION_SYNC_TOPIC, SessionSyncMessage}},
};

/// Démarre un nœud `Persistency` : détenteur de secours durable pour les
/// sessions (voir `ControlPlaneState::persistency_nodes` et
/// `network::cp::reconcile`), qui rejoue les diffs gossipés sur
/// `session::sync::SESSION_SYNC_TOPIC` dans `store` et répond aux demandes
/// [`RpcCall::FETCH_SESSION`] à partir de ce qui y est stocké.
///
/// Ne participe ni au cluster Raft du control plane, ni à l'exécution de
/// jobs — un pair de plus dans le mesh, découvert par le control plane comme
/// `WorkerPeerDiscovered`/`ControlPlanePeerDiscovered` le sont (voir
/// `NetworkEvent::PersistencyPeerDiscovered`).
///
/// `ready` : signalé avec le [`NetworkClient`] de ce nœud dès la connexion
/// établie, avant que la boucle ci-dessous ne démarre — voir
/// `node::Marie::start`.
pub async fn start_persistency(
    secret: Arc<SecretManager>,
    store: Arc<dyn SessionStore>,
    filesystem: SessionFilesystem,
    ready: oneshot::Sender<NetworkClient>,
) -> Result<(), anyhow::Error> {
    use NodeKind::Persistency;

    let swarm = start_swarm(Persistency, |_| {}).await?;
    let (actor, client) = NetworkActor::new(swarm, secret);
    let _ = ready.send(client.clone());

    client.subscribe(SESSION_SYNC_TOPIC);
    let mut events = client.subscribe_events();

    tokio::spawn(actor.run());

    while let Some(event) = events.next().await {
        use NetworkEvent::*;
        match event {
            RequestRemoteProcedureExecution { tx, call, peer: _ } => {
                let res = execute_rpc(call, &store, &filesystem).await;
                let res = match res {
                    Ok(value) => RpcResult::RpcOk(value),
                    Err(error) => RpcResult::RpcErr(error.to_string()),
                };
                // `tx` est partagé (voir `RpcReplySlot`) : un seul abonné doit
                // effectivement répondre, celui qui réussit `.take()` en premier
                // (ici, toujours nous — ce nœud est seul à vouloir répondre).
                if let Ok(mut tx) = tx.lock() {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(res);
                    }
                }
            }
            GossipMessageReceived { topic, data, source } if topic == SESSION_SYNC_TOPIC => {
                if let Err(error) = ingest_session_diff(&store, &client, source, &data).await {
                    warn!(%error, "traitement du diff de session échoué, ignoré");
                }
            }
            // Ce nœud ne participe ni au cluster Raft du control plane, ni à
            // l'exécution de jobs, ni au registre RPC dynamique : seul le gossip
            // sur `SESSION_SYNC_TOPIC` (traité ci-dessus) le concerne.
            ControlPlanePeerDiscovered { .. }
            | WorkerPeerDiscovered { .. }
            | PersistencyPeerDiscovered { .. }
            | PeerDisconnected { .. }
            | GossipMessageReceived { .. } => {}
        }
    }

    Ok(())
}

async fn execute_rpc(call: RpcCall, store: &Arc<dyn SessionStore>, filesystem: &SessionFilesystem) -> Result<serde_json::Value, anyhow::Error> {
    match call.name.as_str() {
        RpcCall::FETCH_SESSION => {
            let request: SessionFetchRequest = serde_json::from_value(call.args)?;
            let remote_sv = StateVector::decode_v1(&request.state_vector).map_err(|error| anyhow::anyhow!(error))?;

            let Some(diff) = store.diff_since(request.session_id, &remote_sv).await? else {
                bail!("session {} inconnue de ce nœud de persistance", request.session_id);
            };

            Ok(serde_json::to_value(diff)?)
        }
        // Suppression définitive : le contenu CRDT (voir `SessionStore`) et
        // tous les fichiers de la session (voir `SessionFilesystem`) — ni
        // l'un ni l'autre n'a de sens à conserver seul une fois la session
        // close.
        RpcCall::DELETE_SESSION => {
            let session_id: SessionId = serde_json::from_value(call.args)?;
            store.delete(&session_id).await?;
            filesystem.delete_session(session_id).await?;
            Ok(serde_json::Value::Null)
        }
        name => bail!("unmanaged remote procedure {name}"),
    }
}

/// Fusionne un diff gossipé sur `SESSION_SYNC_TOPIC` dans le stockage
/// durable. Une session jamais vue localement ne peut pas être reconstruite
/// à partir d'un simple diff incrémental (voir la note sur les racines
/// concurrentes dans `YrsSession::from_diff`) : on récupère alors l'état
/// complet auprès de `source`, le pair qui vient de gossiper ce diff et qui
/// le détient donc forcément.
async fn ingest_session_diff(store: &Arc<dyn SessionStore>, client: &NetworkClient, source: PeerId, data: &[u8]) -> anyhow::Result<()> {
    let message: SessionSyncMessage = serde_json::from_slice(data)?;

    let mut session = match store.get(&message.session_id).await? {
        Some(session) => session,
        None => {
            let request = SessionFetchRequest { session_id: message.session_id, state_vector: StateVector::default().encode_v1() };
            let full_diff: Vec<u8> = client.rpc_to(RpcCall::new(RpcCall::FETCH_SESSION, request), source).await?;
            YrsSession::from_diff(&full_diff)?
        }
    };

    session.apply_diff(&message.diff)?;
    store.put(&message.session_id, &session).await?;
    Ok(())
}
