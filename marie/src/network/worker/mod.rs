pub mod info;
pub mod session_client;

use std::sync::Arc;

use anyhow::bail;
use futures::StreamExt as _;
use libp2p::PeerId;
use tokio::sync::oneshot;
use tracing::warn;

use crate::{
    job::{JobId, JobState},
    network::{
        actor::{NetworkActor, NetworkClient},
        cp::rpc::{JobStateReport, RpcCall, RpcResult, RunJobRequest, SessionFetchRequest, Void},
        peer::NodeKind,
        start_swarm,
    },
    persistency::SessionFilesystem,
    secret::SecretManager,
};
use session_client::SessionClient;

/// `secret` : secret partagé par le cluster, utilisé pour vérifier
/// automatiquement qu'un pair prétendant être control plane l'est vraiment
/// (voir `secret::SecretManager::verify_membership` et
/// `network::actor::NetworkActor`) avant de lui faire confiance et de lui
/// envoyer des jobs.
///
/// `filesystem` : partagé par tous les workers du cluster (voir
/// `persistency::FilesystemConfig`), transmis tel quel à [`SessionClient`].
///
/// `ready` : signalé avec le [`NetworkClient`] de ce nœud dès la connexion
/// établie, avant que la boucle ci-dessous ne démarre — voir
/// `node::Marie::start`.
pub async fn start_worker(secret: Arc<SecretManager>, filesystem: SessionFilesystem, ready: oneshot::Sender<NetworkClient>) -> Result<(), anyhow::Error> {
    use NodeKind::Worker;

    let swarm = start_swarm(Worker, |_| {}).await?;
    let local_peer_id = *swarm.local_peer_id();
    let (actor, client) = NetworkActor::new(swarm, secret);
    let _ = ready.send(client.clone());

    // `SessionClient` s'abonne lui-même au flux d'événements de `client` (voir
    // `NetworkClient::subscribe_events`) pour le gossip qui l'intéresse — un flux
    // indépendant de celui que cette boucle consomme ci-dessous pour répondre aux
    // `RequestRemoteProcedureExecution`.
    let sessions = SessionClient::new(client.clone(), filesystem);
    let mut events = client.subscribe_events();

    tokio::spawn(actor.run());

    while let Some(event) = events.next().await {
        use crate::network::actor::NetworkEvent::*;
        match event {
            RequestRemoteProcedureExecution { tx, call, peer: _ } => {
                let res = execute_rpc(call, &client, &sessions, local_peer_id).await;
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
            // Ce nœud ne participe pas au cluster Raft du control plane, ni au
            // registre RPC dynamique inter-control-planes : ces événements ne
            // concernent que les control planes entre eux. `GossipMessageReceived`
            // est traité indépendamment par `SessionClient` (voir plus haut).
            ControlPlanePeerDiscovered { .. }
            | WorkerPeerDiscovered { .. }
            | PersistencyPeerDiscovered { .. }
            | PeerDisconnected { .. }
            | GossipMessageReceived { .. } => {}
        }
    }

    Ok(())
}

async fn execute_rpc(call: RpcCall, client: &NetworkClient, sessions: &SessionClient, local_peer_id: PeerId) -> Result<serde_json::Value, anyhow::Error> {
    match call.name.as_str() {
        RpcCall::RUN_JOB => {
            let request: RunJobRequest = serde_json::from_value(call.args)?;
            let client = client.clone();
            let sessions = sessions.clone();

            // Exécution en tâche de fond : le control plane n'attend qu'un
            // accusé de réception, pas l'issue du job (qui peut être longue).
            tokio::spawn(execute_and_report(client, sessions, request, local_peer_id));

            Ok(serde_json::Value::Null)
        }
        // Worker -> worker : un pair qui reprend une session dont nous avons la
        // dernière version demande le diff qui lui manque (voir
        // `SessionClient::acquire`). Refusé silencieusement (erreur) si nous ne
        // la détenons pas (plus localement, ou jamais détenue).
        RpcCall::FETCH_SESSION => {
            let request: SessionFetchRequest = serde_json::from_value(call.args)?;
            Ok(serde_json::to_value(sessions.serve_fetch(request).await?)?)
        }
        name => bail!("unmanaged remote procedure {name}"),
    }
}

/// Exécute le job puis rapporte son résultat au control plane.
///
/// Les erreurs de rapport (control plane injoignable, pas leader, etc.) sont
/// loggées mais ne remontent nulle part : c'est un fire-and-forget, cohérent
/// avec la réassignation côté control plane — un job sans nouvelle finira par
/// être détecté et réassigné au prochain healthcheck manqué.
async fn execute_and_report(client: NetworkClient, sessions: SessionClient, request: RunJobRequest, local_peer_id: PeerId) {
    let job_id = request.job.id;

    if let Err(error) = report_job_state(&client, job_id, JobState::Running { worker: local_peer_id }).await {
        warn!(%error, %job_id, "impossible de rapporter le démarrage du job");
    }

    let new_state = match run_job(request, &sessions).await {
        Ok(result) => JobState::Completed { result },
        Err(error) => JobState::Failed { error, retry_count: 0 },
    };

    if let Err(error) = report_job_state(&client, job_id, new_state).await {
        warn!(%error, %job_id, "impossible de rapporter l'issue du job");
    }
}

async fn report_job_state(client: &NetworkClient, job_id: JobId, state: JobState) -> Result<(), anyhow::Error> {
    client.rpc::<Void>(RpcCall::new(RpcCall::REPORT_JOB_STATE, JobStateReport { job_id, state })).await?;
    Ok(())
}

/// Exécute effectivement le job.
///
/// La synchronisation de la session (voir [`SessionClient::acquire`]) est en
/// place — une fois acquise, elle reste à jour en continu via le gossip de
/// [`SessionClient`], sans action supplémentaire ici ; la boucle d'exécution
/// de l'agent elle-même (modèles, outils, voir `agent::run`) dépend d'un
/// catalogue de modèles et d'outils pas encore câblés côté worker, et sera
/// ajoutée séparément.
async fn run_job(request: RunJobRequest, sessions: &SessionClient) -> Result<String, String> {
    use crate::job::JobKind::RunAgent;

    match request.job.kind {
        RunAgent(global_agent_id) => {
            let session_id = global_agent_id.session_id();

            sessions.acquire(session_id, &request.known_holders).await.map_err(|error| error.to_string())?;

            todo!("exécuter l'agent {global_agent_id:?} à partir de la session synchronisée")
        }
    }
}
