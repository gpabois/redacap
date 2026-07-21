pub mod rpc;
pub mod state;
pub mod log_store;
pub mod types;
pub mod network;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Instant, Duration};

use anyhow::bail;
use futures::StreamExt as _;
use libp2p::PeerId;
use openraft::error::{ClientWriteError, ForwardToLeader, RaftError};
use openraft::raft::{AppendEntriesRequest, InstallSnapshotRequest, VoteRequest};
use openraft::{ChangeMembers, Config, Raft};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio::time::{interval, sleep};
use tracing::{debug, info, warn};

use crate::{
    job::{Job, JobId, JobKind, JobState},
    model::{
        catalog::{
            ModelCatalog, ModelId,
            store::{StoredModel, decrypt_from_storage},
        },
        declaration::EncryptedModelDeclaration,
    },
    network::{
        actor::{NetworkActor, NetworkClient},
        cp::{
            log_store::LogStore,
            network::NetworkFactory,
            rpc::{JobStateReport, RpcCall, RpcResult, RunJobRequest, SetModelRequest, SetToolRequest},
            state::{ControlPlaneState, ControlPlaneStateMachineStore},
            types::{ControlPlaneRequest, RaftNode, RaftNodeId, TypeConfig},
        },
        peer::NodeKind,
        start_swarm,
        worker::info::WorkerInfo,
    },
    persistency::store::Store,
    secret::{SecretError, SecretManager},
    session::SessionId,
    tools::catalog::{ToolCatalog, ToolId, store::StoredTool},
};

/// Fenêtre de découverte mDNS/identify avant de figer l'élection du nœud bootstrap.
///
/// À l'expiration de ce délai, chaque nœud `ControlPlane` calcule *localement*
/// et *sans aucun message d'élection* lequel des pairs connus (lui compris) a
/// le `node_id` le plus faible, et considère ce pair comme le nœud bootstrap.
/// Voir [`elect_bootstrap_leader`].
const BOOTSTRAP_DELAY: Duration = Duration::from_secs(3);

pub struct NodeHealth {
    pub last_seen: Instant,
    pub rtt: Option<Duration>, // Round-Trip Time (latence)
    pub status: NodeStatus,
}

pub enum NodeStatus {
    Alive,
    Dead
}

/// Topic gossipsub (`node_gossip`) sur lequel les nœuds `ControlPlane` se
/// tiennent mutuellement informés des enregistrements RPC dynamiques — voir
/// [`DynamicRpcRegistry`] et [`RpcRegistryGossip`].
const RPC_REGISTRY_TOPIC: &str = "marie/cp/rpc-registry/1.0.0";

/// Message gossipé entre nœuds `ControlPlane` pour propager les
/// enregistrements RPC dynamiques appris directement (voir
/// `RpcCall::REGISTER_RPC`) à tout le cluster de control planes, même à ceux
/// qui n'ont pas de connexion directe avec l'exécuteur concerné.
///
/// Limite assumée : si le nœud à l'origine de l'enregistrement disparaît sans
/// avoir pu gossiper l'`Unregister` correspondant (crash plutôt qu'arrêt
/// propre), les autres nœuds gardent une entrée périmée jusqu'à ce qu'un
/// relais échoué la purge (voir l'auto-guérison dans `execute_rpc`). Pas de
/// TTL/heartbeat ici : jugé disproportionné pour ce cas d'usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum RpcRegistryGossip {
    Register { name: String, peer_id: PeerId },
    Unregister { name: String, peer_id: PeerId },
}

/// Registre des noms de RPC enregistrés dynamiquement par des pairs
/// volontaires pour les exécuter (voir `NetworkClient::register_rpc`).
///
/// Local à ce nœud control plane — pas répliqué par Raft (voir
/// [`RpcRegistryGossip`] pour la justification et le mécanisme de propagation
/// retenu à la place). Alimenté par les enregistrements directs (pairs
/// connectés à ce nœud) et par le gossip des autres control planes.
#[derive(Default)]
struct DynamicRpcRegistry {
    executors: HashMap<String, HashSet<PeerId>>,
}

impl DynamicRpcRegistry {
    /// Enregistre `peer_id` comme exécuteur de `name`. Retourne `true` si
    /// c'est une nouveauté (à gossiper), `false` si déjà connu.
    fn register(&mut self, name: String, peer_id: PeerId) -> bool {
        self.executors.entry(name).or_default().insert(peer_id)
    }

    /// Applique un message gossipé par un autre control plane : n'est jamais
    /// re-gossipé (évite les boucles).
    fn apply_gossip(&mut self, msg: RpcRegistryGossip) {
        match msg {
            RpcRegistryGossip::Register { name, peer_id } => {
                self.executors.entry(name).or_default().insert(peer_id);
            }
            RpcRegistryGossip::Unregister { name, peer_id } => self.remove_executor(&name, &peer_id),
        }
    }

    /// Retire `peer_id` de toutes les RPC qu'il exécutait — utilisé quand ce
    /// nœud perd sa propre connexion vers lui. Retourne les noms concernés,
    /// à gossiper en `Unregister` (seul le nœud ayant observé la déconnexion
    /// peut le faire).
    fn remove_peer(&mut self, peer_id: &PeerId) -> Vec<String> {
        let mut affected = Vec::new();
        self.executors.retain(|name, peers| {
            if peers.remove(peer_id) {
                affected.push(name.clone());
            }
            !peers.is_empty()
        });
        affected
    }

    /// Retire `peer_id` de la liste des exécuteurs de `name` uniquement
    /// (contrairement à [`Self::remove_peer`], ne touche pas ses autres
    /// enregistrements). Utilisé pour l'auto-guérison locale d'une entrée
    /// dont un relais vient d'échouer — volontairement non re-gossipé, voir
    /// `execute_rpc`.
    fn remove_executor(&mut self, name: &str, peer_id: &PeerId) {
        if let Some(peers) = self.executors.get_mut(name) {
            peers.remove(peer_id);
            if peers.is_empty() {
                self.executors.remove(name);
            }
        }
    }

    /// Exécuteurs actuellement enregistrés pour `name`, s'il y en a au moins un.
    fn executors_for(&self, name: &str) -> Option<&HashSet<PeerId>> {
        self.executors.get(name).filter(|peers| !peers.is_empty())
    }
}

/// Relaie `call` vers tous les `executors` en parallèle et retourne la
/// première réponse positive — "le premier qui répond l'emporte". Les autres
/// requêtes en vol sont abandonnées (leur future est simplement annulée par
/// `select_ok` en étant droppée).
async fn forward_race(
    client: &NetworkClient,
    executors: &HashSet<PeerId>,
    call: RpcCall,
) -> Result<serde_json::Value, anyhow::Error> {
    type Attempt = Pin<Box<dyn Future<Output = Result<serde_json::Value, anyhow::Error>> + Send>>;

    let attempts: Vec<Attempt> = executors
        .iter()
        .map(|&peer_id| {
            let client = client.clone();
            let call = call.clone();
            Box::pin(async move { client.rpc_to::<serde_json::Value>(call, peer_id).await }) as Attempt
        })
        .collect();

    let (value, _still_pending) = futures::future::select_ok(attempts).await?;
    Ok(value)
}

/// Reconstitue le catalogue de modèles depuis le stockage chiffré local (voir
/// `model::catalog::store`) — utilisé pour la récupération à froid au
/// démarrage (voir [`start_control_plane`]). Best-effort : une entrée
/// illisible (déchiffrement échoué) ou une lecture du stockage en échec sont
/// journalisées puis ignorées plutôt que de bloquer le démarrage — dans le
/// pire cas, le catalogue démarre incomplet ou vide et se repeuple via Raft
/// en rejoignant le cluster.
async fn load_catalog_from_store(model_store: &Arc<dyn Store<StoredModel>>, secret: &SecretManager) -> ModelCatalog {
    let mut catalog = ModelCatalog::default();

    let stored_models = match model_store.list().await {
        Ok(stored_models) => stored_models,
        Err(error) => {
            warn!(%error, "lecture du catalogue de modèles local impossible, catalogue vide au démarrage (récupération attendue depuis Raft)");
            return catalog;
        }
    };

    for stored in stored_models {
        match decrypt_from_storage(&stored.declaration, secret) {
            Ok(declaration) => {
                catalog.insert(stored.id, declaration);
            }
            Err(error) => warn!(%error, id = %stored.id, "déchiffrement d'un modèle stocké localement impossible, ignoré"),
        }
    }

    catalog
}

/// Reconstitue le catalogue de tools depuis le stockage local (voir
/// `tools::catalog::store`) — utilisé pour la récupération à froid au
/// démarrage (voir [`start_control_plane`]), sur le même modèle que
/// [`load_catalog_from_store`] mais sans déchiffrement (voir
/// [`crate::tools::declaration::ToolDeclaration`]).
async fn load_tool_catalog_from_store(tool_store: &Arc<dyn Store<StoredTool>>) -> ToolCatalog {
    let mut catalog = ToolCatalog::default();

    let stored_tools = match tool_store.list().await {
        Ok(stored_tools) => stored_tools,
        Err(error) => {
            warn!(%error, "lecture du catalogue de tools local impossible, catalogue vide au démarrage (récupération attendue depuis Raft)");
            return catalog;
        }
    };

    for stored in stored_tools {
        catalog.insert(stored.id, stored.declaration);
    }

    catalog
}

/// Dérive un identifiant Raft numérique stable à partir du `PeerId` libp2p local.
///
/// Note : le `PeerId` change à chaque démarrage (identité générée via
/// `with_new_identity()`), donc ce `node_id` ne survit pas à un redémarrage.
/// Suffisant pour l'instant en l'absence de persistance d'identité.
fn derive_node_id(peer_id: &PeerId) -> RaftNodeId {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    peer_id.hash(&mut hasher);
    hasher.finish()
}

/// Intervalle du cycle de contrôle périodique (healthcheck + ordonnancement +
/// réassignation) — voir [`reconcile`].
const RECONCILE_INTERVAL: Duration = Duration::from_secs(4);

/// Nombre de tentatives (essai initial compris) avant d'abandonner un relais
/// RPC (vers le leader raft ou vers un exécuteur enregistré dynamiquement)
/// dont la cible s'avère injoignable — voir [`propose_or_forward`] et le
/// relais dynamique dans [`execute_rpc`].
const FORWARD_RETRY_ATTEMPTS: u32 = 3;
/// Délai entre deux tentatives de relais — laisse le temps à une élection
/// raft de converger, ou à un exécuteur de repli de se signaler.
const FORWARD_RETRY_DELAY: Duration = Duration::from_millis(300);

/// `secret` : secret partagé par le cluster, utilisé pour prouver
/// automatiquement l'appartenance de ce nœud aux autres control planes lors de
/// la découverte réseau (voir `secret::SecretManager::prove_membership` et
/// `network::actor::NetworkActor`) — sans lui, aucun pair ne reconnaîtrait ce
/// nœud comme control plane, et il ne rejoindrait jamais le cluster Raft.
///
/// `model_store` : stockage chiffré local du catalogue de modèles (voir
/// `model::catalog::store`). Au démarrage, sert de première source pour
/// peupler `ControlPlaneState::models` (voir [`load_catalog_from_store`]) —
/// une récupération à froid immédiate, sans dépendre du reste du cluster. Si
/// ce nœud n'a jamais rien persisté (premier démarrage, ou stockage vide),
/// le catalogue démarre vide et se peuple normalement via Raft en rejoignant
/// le cluster (réplication des entrées de log, ou snapshot complet si ce
/// nœud a trop de retard — voir `ControlPlaneStateMachineStore::install_snapshot`).
///
/// `tool_store` : équivalent de `model_store` pour `ControlPlaneState::tools`
/// (voir [`load_tool_catalog_from_store`]) — pas de chiffrement, une
/// déclaration de tool ne porte aucun secret.
///
/// `ready` : signalé avec le [`NetworkClient`] de ce nœud dès la connexion
/// établie, avant que la boucle ci-dessous ne démarre — permet à l'appelant
/// (voir `node::Marie::start`) de le récupérer sans attendre l'arrêt du
/// nœud, qui ne survient normalement jamais.
pub async fn start_control_plane(
    secret: Arc<SecretManager>,
    model_store: Arc<dyn Store<StoredModel>>,
    tool_store: Arc<dyn Store<StoredTool>>,
    ready: oneshot::Sender<NetworkClient>,
) -> Result<(), anyhow::Error> {
    use NodeKind::ControlPlane;

    let log_store = LogStore::new(); // stocke le log
    let initial_models = load_catalog_from_store(&model_store, &secret).await;
    let initial_tools = load_tool_catalog_from_store(&tool_store).await;
    let initial_state = ControlPlaneState { models: initial_models, tools: initial_tools, ..Default::default() };
    let state_machine = ControlPlaneStateMachineStore::new(initial_state, model_store, tool_store, secret.clone()); // applique le log

    let mut reconcile_timer = interval(RECONCILE_INTERVAL);

    let swarm = start_swarm(ControlPlane, |_| {}).await?;
    let local_peer_id = *swarm.local_peer_id();
    let node_id = derive_node_id(&local_peer_id);
    let (actor, client) = NetworkActor::new(swarm, secret.clone());
    let _ = ready.send(client.clone());
    let mut events = client.subscribe_events();

    let network_factory = NetworkFactory::new(client.clone());
    let config = Arc::new(Config::default().validate()?);

    let raft = Raft::new(node_id, config, network_factory, log_store, state_machine.clone()).await?;

    tokio::spawn(actor.run());
    client.subscribe(RPC_REGISTRY_TOPIC);

    // Membres connus pour le bootstrap initial du cluster (voir `BOOTSTRAP_DELAY`).
    let mut known_members: BTreeMap<RaftNodeId, RaftNode> = BTreeMap::new();
    known_members.insert(node_id, RaftNode { peer_id: Some(local_peer_id), addr: String::new() });
    let mut bootstrapped = false;
    let bootstrap_delay = tokio::time::sleep(BOOTSTRAP_DELAY);
    tokio::pin!(bootstrap_delay);

    // État local (non répliqué) du scheduler : santé des workers connus et
    // job actuellement assigné à chacun. Reconstruit au fil des healthchecks
    // et de l'état Raft — perdu à chaque redémarrage, sans conséquence sur la
    // correction puisqu'il n'est qu'un cache d'ordonnancement, pas une source
    // de vérité (celle-ci reste `ControlPlaneState`, répliquée par Raft).
    let mut health: HashMap<PeerId, NodeHealth> = HashMap::new();
    let mut assignments: HashMap<JobId, PeerId> = HashMap::new();
    let mut rpc_registry = DynamicRpcRegistry::default();

    loop {
        tokio::select! {
            Some(event) = events.next() => {
                use crate::network::actor::NetworkEvent::*;
                match event {
                    RequestRemoteProcedureExecution { tx, call, peer } => {

                        let res = execute_rpc(call, &state_machine, &client, &raft, &secret, local_peer_id, &mut rpc_registry, peer).await;
                        let res = match res {
                            Ok(value) => RpcResult::RpcOk(value),
                            Err(error) => RpcResult::RpcErr(error.to_string()),
                        };
                        // `tx` est partagé (voir `RpcReplySlot`) : un seul abonné à
                        // `NetworkEvent` doit effectivement répondre, celui qui réussit
                        // `.take()` en premier (ici, toujours nous — ce nœud est seul à
                        // vouloir répondre aux RPC entrantes).
                        if let Ok(mut tx) = tx.lock() {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(res);
                            }
                        }
                    },
                    ControlPlanePeerDiscovered { peer_id, addr } => {
                        let peer_node_id = derive_node_id(&peer_id);
                        let peer_node = RaftNode { peer_id: Some(peer_id), addr: addr.map(|a| a.to_string()).unwrap_or_default() };

                        let is_new = known_members.insert(peer_node_id, peer_node.clone()).is_none();

                        // Le bootstrap initial (ci-dessous) se charge déjà des pairs connus
                        // avant `BOOTSTRAP_DELAY`. Une fois le cluster démarré, tout nouveau
                        // pair doit être rattaché dynamiquement.
                        if is_new && bootstrapped {
                            sync_new_peer(&raft, peer_node_id, peer_node).await;
                        }
                    },
                    WorkerPeerDiscovered { peer_id, .. } => {
                        health.entry(peer_id).or_insert_with(|| NodeHealth {
                            last_seen: Instant::now(),
                            rtt: None,
                            status: NodeStatus::Alive,
                        });

                        // Répliqué via Raft : ignoré silencieusement si ce nœud n'est pas
                        // leader, le leader effectif le fera à sa propre découverte du pair.
                        propose_best_effort(&raft, ControlPlaneRequest::RegisterWorker {
                            worker: WorkerInfo { peer_id },
                        }).await;
                    },
                    PersistencyPeerDiscovered { peer_id, .. } => {
                        // Répliqué via Raft, comme `RegisterWorker` : ignoré silencieusement
                        // si ce nœud n'est pas leader, le leader effectif le fera à sa propre
                        // découverte du pair. Voir `ControlPlaneState::persistency_nodes`.
                        propose_best_effort(&raft, ControlPlaneRequest::RegisterPersistency { peer_id }).await;
                    },
                    PeerDisconnected { peer_id } => {
                        // "Si tous les nœuds se déconnectent, cela retire le RPC" : `remove_peer`
                        // ne laisse subsister que les RPC ayant encore au moins un exécuteur.
                        // On gossipe le retrait pour que les autres control planes (qui
                        // n'observent pas forcément la même déconnexion) se mettent à jour aussi.
                        for name in rpc_registry.remove_peer(&peer_id) {
                            let msg = RpcRegistryGossip::Unregister { name, peer_id };
                            let _ = client.publish(RPC_REGISTRY_TOPIC, msg);
                        }
                    },
                    GossipMessageReceived { topic, data, .. } => {
                        if topic == RPC_REGISTRY_TOPIC {
                            if let Ok(msg) = serde_json::from_slice::<RpcRegistryGossip>(&data) {
                                rpc_registry.apply_gossip(msg);
                            }
                        }
                    },
                }
            }
            () = &mut bootstrap_delay, if !bootstrapped => {
                bootstrapped = true;

                if elect_bootstrap_leader(node_id, &known_members) == node_id {
                    info!(
                        node_id,
                        pairs = known_members.len(),
                        "élu nœud bootstrap Raft (node_id le plus faible parmi les pairs connus) — initialisation du cluster"
                    );
                    if let Err(error) = raft.initialize(known_members.clone()).await {
                        debug!(%error, "initialisation raft ignorée (cluster déjà démarré entre-temps)");
                    }
                } else {
                    info!(
                        node_id,
                        "non élu nœud bootstrap — en attente d'être rattaché au cluster par le nœud élu"
                    );
                }
            }
            _ = reconcile_timer.tick() => {
                reconcile(&raft, &client, &state_machine, &mut health, &mut assignments, node_id).await;
            }
        }
    }
}

/// Cycle de contrôle périodique du control plane :
///
/// 1. Healthcheck de tous les workers enregistrés (connectivité libp2p —
///    aucun handler applicatif requis côté worker).
/// 2. Si ce nœud est actuellement leader : remise en attente des jobs dont le
///    worker vient d'être détecté injoignable (réassignation).
/// 3. Assignation des jobs `Pending` aux workers vivants et disponibles, avec
///    notification best-effort du worker via [`RpcCall::RUN_JOB`].
///
/// Les étapes 2 et 3 écrivent dans l'état répliqué via [`propose_best_effort`],
/// qui échoue silencieusement si ce nœud n'est pas leader — c'est pourquoi
/// elles sont sautées explicitement plus tôt : inutile de calculer des
/// décisions d'ordonnancement qui seront de toute façon rejetées.
async fn reconcile(
    raft: &Raft<TypeConfig>,
    client: &NetworkClient,
    state_machine: &ControlPlaneStateMachineStore,
    health: &mut HashMap<PeerId, NodeHealth>,
    assignments: &mut HashMap<JobId, PeerId>,
    node_id: RaftNodeId,
) {
    let state = state_machine.read_state().await;

    let mut newly_dead = Vec::new();
    for peer_id in state.workers.keys().copied() {
        let alive = client.is_connected(peer_id).await.unwrap_or(false);
        let was_alive = health.get(&peer_id).is_none_or(|h| matches!(h.status, NodeStatus::Alive));

        let entry = health.entry(peer_id).or_insert_with(|| NodeHealth {
            last_seen: Instant::now(),
            rtt: None,
            status: NodeStatus::Alive,
        });
        entry.status = if alive { NodeStatus::Alive } else { NodeStatus::Dead };
        if alive {
            entry.last_seen = Instant::now();
        } else if was_alive {
            newly_dead.push(peer_id);
        }
    }

    if raft.current_leader().await != Some(node_id) {
        return;
    }

    for dead_peer in newly_dead {
        let orphaned: Vec<JobId> =
            assignments.iter().filter(|(_, worker)| **worker == dead_peer).map(|(job_id, _)| *job_id).collect();

        for job_id in orphaned {
            assignments.remove(&job_id);
            debug!(%job_id, %dead_peer, "worker injoignable, remise en attente du job pour réassignation");
            propose_best_effort(raft, ControlPlaneRequest::CommitState { job_id, new_state: JobState::Pending }).await;
        }
    }

    let busy: HashSet<PeerId> = assignments.values().copied().collect();
    let mut available_workers = state.workers.keys().copied().filter(|peer_id| {
        !busy.contains(peer_id) && matches!(health.get(peer_id).map(|h| &h.status), Some(NodeStatus::Alive))
    });

    for (job_id, record) in state.jobs.iter().filter(|(_, record)| matches!(record.state, JobState::Pending)) {
        let Some(worker) = available_workers.next() else { break };

        assignments.insert(*job_id, worker);
        propose_best_effort(raft, ControlPlaneRequest::AssignJob { job_id: *job_id, worker }).await;

        // Le worker assigné n'est pas garanti d'être celui qui exécutait déjà cette
        // session (réassignation après un healthcheck manqué, ou simplement un
        // nouveau frame de la même session parti sur un autre worker) : on lui
        // indique les détenteurs actuellement connus de son état CRDT pour qu'il
        // puisse s'y synchroniser avant de reprendre — voir
        // `ControlPlaneState::session_holders` et `RpcCall::FETCH_SESSION`.
        let known_holders = session_id_of(&record.job).map(|id| session_holders_for(&state, assignments, id)).unwrap_or_default();
        let request = RunJobRequest { job: record.job.clone(), known_holders };
        let call = RpcCall::new(RpcCall::RUN_JOB, request);
        if let Err(error) = client.rpc_to::<serde_json::Value>(call, worker).await {
            debug!(%error, %job_id, %worker, "notification 'run-job' échouée (le worker n'a peut-être pas encore le handler)");
        }
    }
}

/// Session ciblée par `job`, le cas échéant (dépend de `JobKind`).
fn session_id_of(job: &Job) -> Option<SessionId> {
    match &job.kind {
        JobKind::RunAgent(global_agent_id) => Some(global_agent_id.session_id()),
    }
}

/// Combine les détenteurs connus via l'état Raft
/// (`ControlPlaneState::session_holders`) et ceux tout juste assignés plus
/// tôt dans ce même passage de `reconcile`, via le cache local `assignments`
/// — pas encore visibles dans `state`, puisque la proposition Raft
/// correspondante (voir `propose_best_effort`) est asynchrone. Sans cette
/// combinaison, deux frames d'une même session assignés au même tick à des
/// workers différents ne se verraient pas l'un l'autre et créeraient chacun
/// une session CRDT vierge et divergente.
///
/// Les nœuds `Persistency` connus (voir `ControlPlaneState::persistency_nodes`)
/// sont ajoutés en fin de liste : `SessionClient::acquire` les essaie dans
/// l'ordre, donc les workers vivants (état le plus frais) passent avant ce
/// détenteur de secours, consulté seulement si aucun d'eux ne répond (ou
/// qu'aucun n'est actif — ex: reprise d'une session entre deux jobs).
fn session_holders_for(state: &ControlPlaneState, assignments: &HashMap<JobId, PeerId>, session_id: SessionId) -> Vec<PeerId> {
    let mut holders = state.session_holders(session_id);
    for (job_id, worker) in assignments {
        if state.jobs.get(job_id).and_then(|record| session_id_of(&record.job)) == Some(session_id) {
            holders.insert(*worker);
        }
    }

    let mut ordered: Vec<PeerId> = holders.iter().copied().collect();
    for &peer_id in &state.persistency_nodes {
        if !holders.contains(&peer_id) {
            ordered.push(peer_id);
        }
    }
    ordered
}

/// Élit, de façon déterministe et sans message d'élection, le nœud bootstrap
/// parmi un ensemble de membres connus : celui dont le `node_id` est le plus
/// faible.
///
/// Cette règle ne fonctionne que si tous les nœuds `ControlPlane` convergent
/// vers (approximativement) le même ensemble de pairs pendant `BOOTSTRAP_DELAY`
/// (vrai en pratique sur un même LAN via mDNS, où la découverte est
/// symétrique). Un pair que le nœud élu n'aurait pas encore découvert à ce
/// moment-là rejoint quand même le cluster dès que l'élu le découvre à son
/// tour, via [`sync_new_peer`].
fn elect_bootstrap_leader(local_node_id: RaftNodeId, known_members: &BTreeMap<RaftNodeId, RaftNode>) -> RaftNodeId {
    known_members.keys().copied().min().unwrap_or(local_node_id)
}

/// Rattache un pair `ControlPlane` découvert après le bootstrap initial : d'abord
/// comme learner (réplication du log), puis promu voter. Échoue silencieusement
/// si ce nœud n'est pas (ou plus) leader — c'est alors au leader courant de le faire
/// lorsqu'il recevra le même événement de découverte.
async fn sync_new_peer(raft: &Raft<TypeConfig>, node_id: RaftNodeId, node: RaftNode) {
    if let Err(error) = raft.add_learner(node_id, node, true).await {
        debug!(%error, node_id, "impossible d'ajouter le pair comme learner (probablement pas leader)");
        return;
    }

    if let Err(error) = raft.change_membership(ChangeMembers::AddVoterIds(BTreeSet::from([node_id])), true).await {
        debug!(%error, node_id, "impossible de promouvoir le pair en voter");
    }
}

/// Propose une commande au state machine via Raft, sans garantir qu'elle
/// aboutisse : échoue silencieusement (loggé en `debug`) si ce nœud n'est pas
/// leader. Réservé aux écritures déclenchées en interne (découverte de pair,
/// ordonnancement, réassignation) où aucun appelant RPC n'attend de réponse
/// définitive — le leader effectif, recevant le même déclencheur, retentera
/// l'opération de son côté.
async fn propose_best_effort(raft: &Raft<TypeConfig>, request: ControlPlaneRequest) {
    if let Err(error) = raft.client_write(request).await {
        debug!(%error, "écriture raft ignorée (probablement pas leader)");
    }
}

/// Propose une commande au state machine via Raft en réponse à un appel RPC
/// entrant. Contrairement à [`propose_best_effort`], un appelant attend une
/// réponse définitive : si ce nœud n'est pas leader, l'appel original est
/// transféré au leader connu.
///
/// Si ce leader s'avère injoignable (déconnecté — voir la gestion de
/// `OutboundFailure` dans `NetworkActor`), la RPC doit échouer côté transport
/// avant que l'on décide de retenter : jusqu'à [`FORWARD_RETRY_ATTEMPTS`]
/// essais, chacun réinterrogeant `raft.client_write` (donc le leader courant,
/// qui peut avoir changé entre deux essais — élection en cours, ou ce nœud
/// lui-même vient de le devenir).
async fn propose_or_forward(
    raft: &Raft<TypeConfig>,
    client: &NetworkClient,
    call: RpcCall,
    request: ControlPlaneRequest,
) -> Result<serde_json::Value, anyhow::Error> {
    let mut last_error = None;

    for attempt in 0..FORWARD_RETRY_ATTEMPTS {
        if attempt > 0 {
            sleep(FORWARD_RETRY_DELAY).await;
        }

        match raft.client_write(request.clone()).await {
            Ok(resp) => return Ok(serde_json::to_value(resp.data)?),
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(ForwardToLeader { leader_node: Some(leader), .. }))) => {
                let Some(peer_id) = leader.peer_id else {
                    bail!("le leader raft connu n'a pas de peer_id libp2p");
                };
                match client.rpc_to(call.clone(), peer_id).await {
                    Ok(value) => return Ok(value),
                    Err(error) => {
                        debug!(%error, attempt, "relais vers le leader raft échoué, nouvel essai");
                        last_error = Some(error);
                    }
                }
            }
            Err(error) => bail!(error.to_string()),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("aucun leader raft joignable après {FORWARD_RETRY_ATTEMPTS} tentatives")))
}

async fn execute_rpc(
    call: RpcCall,
    state_machine: &ControlPlaneStateMachineStore,
    client: &NetworkClient,
    raft: &Raft<TypeConfig>,
    secret: &SecretManager,
    local_peer_id: PeerId,
    rpc_registry: &mut DynamicRpcRegistry,
    peer: PeerId,
) -> Result<serde_json::Value, anyhow::Error> {
    match call.name.as_str() {
        RpcCall::REGISTER_RPC => {
            let name: String = serde_json::from_value(call.args)?;
            info!(%peer, rpc = %name, "RPC enregistrée dynamiquement");
            if rpc_registry.register(name.clone(), peer) {
                let msg = RpcRegistryGossip::Register { name, peer_id: peer };
                let _ = client.publish(RPC_REGISTRY_TOPIC, msg);
            }
            Ok(serde_json::Value::Null)
        }
        RpcCall::AUTH_CHALLENGE => {
            let nonce: [u8; 32] = serde_json::from_value(call.args)?;
            let proof = secret.prove_membership(&local_peer_id, &nonce)?;
            Ok(serde_json::to_value(proof)?)
        }
        RpcCall::GET_MODEL => {
            let model_id: ModelId = serde_json::from_value(call.args)?;
            let decl = state_machine.read_state().await.models.get(&model_id).cloned();

            // La clé API ne doit jamais transiter en clair : chiffrée
            // spécifiquement pour `peer` (voir `SecretManager::derive_node_key`),
            // seul ce nœud pourra la déchiffrer (voir
            // `NetworkClient::decrypt_secret`).
            let encrypted = decl.map(|decl| {
                let node_key = secret.derive_node_key(&peer)?;
                let api_key = secret.encrypt_api_key(&decl.api_key, &node_key)?;
                Ok::<_, SecretError>(decl.encrypt(api_key))
            }).transpose()?;

            Ok(serde_json::to_value(encrypted)?)
        }
        RpcCall::LIST_MODELS => {
            let state = state_machine.read_state().await;
            let node_key = secret.derive_node_key(&peer)?;

            let models = state.models.iter().map(|(id, decl)| {
                let api_key = secret.encrypt_api_key(&decl.api_key, &node_key)?;
                Ok::<_, SecretError>((id.clone(), decl.encrypt(api_key)))
            }).collect::<Result<Vec<(ModelId, EncryptedModelDeclaration)>, _>>()?;

            Ok(serde_json::to_value(models)?)
        }
        RpcCall::SET_MODEL => {
            let request: SetModelRequest = serde_json::from_value(call.args.clone())?;
            let cp_request = ControlPlaneRequest::SetModel { id: request.id, declaration: request.declaration };
            propose_or_forward(raft, client, call, cp_request).await
        }
        RpcCall::REMOVE_MODEL => {
            let id: ModelId = serde_json::from_value(call.args.clone())?;
            propose_or_forward(raft, client, call, ControlPlaneRequest::RemoveModel { id }).await
        }
        RpcCall::GET_TOOL => {
            let tool_id: ToolId = serde_json::from_value(call.args)?;
            let decl = state_machine.read_state().await.tools.get(&tool_id).cloned();
            Ok(serde_json::to_value(decl)?)
        }
        RpcCall::LIST_TOOLS => {
            let state = state_machine.read_state().await;
            Ok(serde_json::to_value(&*state.tools)?)
        }
        RpcCall::SET_TOOL => {
            let request: SetToolRequest = serde_json::from_value(call.args.clone())?;
            let cp_request = ControlPlaneRequest::SetTool { id: request.id, declaration: request.declaration };
            propose_or_forward(raft, client, call, cp_request).await
        }
        RpcCall::REMOVE_TOOL => {
            let id: ToolId = serde_json::from_value(call.args.clone())?;
            propose_or_forward(raft, client, call, ControlPlaneRequest::RemoveTool { id }).await
        }
        RpcCall::APPEND_ENTRIES => {
            let rpc: AppendEntriesRequest<TypeConfig> = serde_json::from_value(call.args)?;
            let resp = raft.append_entries(rpc).await.map_err(|error| anyhow::anyhow!(error.to_string()))?;
            Ok(serde_json::to_value(resp)?)
        }
        RpcCall::INSTALL_SNAPSHOT => {
            let rpc: InstallSnapshotRequest<TypeConfig> = serde_json::from_value(call.args)?;
            let resp = raft.install_snapshot(rpc).await.map_err(|error| anyhow::anyhow!(error.to_string()))?;
            Ok(serde_json::to_value(resp)?)
        }
        RpcCall::VOTE => {
            let rpc: VoteRequest<RaftNodeId> = serde_json::from_value(call.args)?;
            let resp = raft.vote(rpc).await.map_err(|error| anyhow::anyhow!(error.to_string()))?;
            Ok(serde_json::to_value(resp)?)
        }
        RpcCall::SUBMIT_JOB => {
            let job: Job = serde_json::from_value(call.args.clone())?;
            propose_or_forward(raft, client, call, ControlPlaneRequest::SubmitJob(job)).await
        }
        RpcCall::REPORT_JOB_STATE => {
            let report: JobStateReport = serde_json::from_value(call.args.clone())?;
            let request = ControlPlaneRequest::CommitState { job_id: report.job_id, new_state: report.state };
            propose_or_forward(raft, client, call, request).await
        }
        name => {
            // Pas une RPC connue nativement : peut-être une RPC enregistrée
            // dynamiquement par un pair (voir `RpcCall::REGISTER_RPC`).
            let name = name.to_string();
            let mut last_error = None;

            for attempt in 0..FORWARD_RETRY_ATTEMPTS {
                if attempt > 0 {
                    sleep(FORWARD_RETRY_DELAY).await;
                }

                // Requêté à chaque essai : reflète la purge de l'essai précédent
                // (ci-dessous) ainsi que tout nouvel exécuteur apparu entre-temps
                // (enregistrement direct ou gossip d'un autre control plane).
                let Some(executors) = rpc_registry.executors_for(&name).cloned() else {
                    bail!("unmanaged remote procedure {name}");
                };

                match forward_race(client, &executors, call.clone()).await {
                    Ok(value) => return Ok(value),
                    Err(error) => {
                        // Aucun exécuteur n'a répondu : probablement des entrées
                        // périmées (apprises par gossip d'un nœud control plane
                        // depuis disparu sans avoir pu gossiper leur retrait — voir
                        // `RpcRegistryGossip`). On les purge localement plutôt que de
                        // rester bloqué dessus. Pas re-gossipé : un échec de relais
                        // isolé n'est pas une preuve aussi définitive qu'une
                        // déconnexion observée directement.
                        debug!(%error, %name, attempt, "relais RPC dynamique échoué, nouvel essai");
                        for peer_id in &executors {
                            rpc_registry.remove_executor(&name, peer_id);
                        }
                        last_error = Some(error);
                    }
                }
            }

            Err(last_error.unwrap_or_else(|| anyhow::anyhow!("unmanaged remote procedure {name}")))
        }
    }
}
