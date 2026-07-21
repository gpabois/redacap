use std::collections::HashMap;
use std::sync::Arc;

use anyhow::bail;
use futures::{Stream, StreamExt as _};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use crate::id::ID;
use tokio::sync::{RwLock, broadcast};
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tracing::debug;
use yrs::{StateVector, updates::{decoder::Decode, encoder::Encode}};

use crate::{
    agent::status::AgentStatus,
    network::{actor::{NetworkClient, NetworkEvent, NetworkEventHandler}, cp::rpc::{RpcCall, SessionFetchRequest}},
    persistency::SessionFilesystem,
    session::{SessionId, SessionLog, crdt::YrsSession, sync::{SESSION_SYNC_TOPIC, SessionSyncMessage}},
};

/// Capacité du canal de diffusion locale des [`SessionEvent`] — des
/// événements de cycle de vie, pas un flux de contenu streamé (voir
/// `agent_events` côté serveur d'édition, bien plus verbeux) : une capacité
/// modeste suffit à laisser un abonnant temporairement en retard rattraper
/// son retard sans perdre d'événement.
const SESSION_EVENTS_CAPACITY: usize = 256;

/// Topic gossipsub (`node_gossip`) sur lequel les événements de session sont
/// diffusés à tout pair intéressé (autre worker préparant une reprise,
/// control plane, outillage d'observation) — voir [`SessionClient::emit`] et
/// [`SessionClient::new`]. Ne transporte que des événements de cycle de vie
/// (petits, peu fréquents), jamais le contenu de la session elle-même (voir
/// `session::sync::SESSION_SYNC_TOPIC` pour ça).
const SESSION_EVENTS_TOPIC: &str = "marie/worker/session-events/1.0.0";

/// Événement de cycle de vie d'une session, diffusé localement (voir
/// [`SessionClient::subscribe`]) et gossipé au reste du cluster (voir
/// [`SESSION_EVENTS_TOPIC`]). Permet de suivre l'avancement d'un agent ou la
/// vie d'une session sans avoir à ré-interroger le CRDT à chaque tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    /// La session est désormais connue localement — créée vierge ou
    /// synchronisée depuis un détenteur précédent (voir
    /// [`SessionClient::acquire`]).
    Created { session_id: SessionId },
    /// Le statut d'un frame de la session vient de changer (voir
    /// [`SessionClient::set_frame_status`]).
    FrameStatusChanged { session_id: SessionId, local_id: ID, status: AgentStatus },
    /// Une entrée a été ajoutée au journal de la session (voir
    /// [`SessionClient::push_log`]).
    LogAppended { session_id: SessionId, log: SessionLog },
    /// La session n'est plus détenue localement par ce worker (voir
    /// [`SessionClient::remove`]).
    Removed { session_id: SessionId },
}

/// Flux de [`SessionEvent`] retourné par [`SessionClient::subscribe`] —
/// encapsule le `broadcast::Receiver` sous-jacent (même motif que
/// `network::actor::NetworkEventHandler` pour `NetworkEvent`) : un abonné en
/// retard perd les événements les plus anciens (`Lagged`), absorbé
/// silencieusement ici plutôt que remonté comme une erreur — un événement de
/// cycle de vie manqué n'est jamais fatal (voir [`SESSION_SYNC_TOPIC`] pour
/// la synchronisation du contenu, qui elle ne dépend pas de ce flux).
pub struct SessionEventHandler(BroadcastStream<SessionEvent>);

impl Stream for SessionEventHandler {
    type Item = SessionEvent;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        loop {
            return match std::pin::Pin::new(&mut self.0).poll_next(cx) {
                std::task::Poll::Ready(Some(Ok(event))) => std::task::Poll::Ready(Some(event)),
                std::task::Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(skipped)))) => {
                    debug!(skipped, "abonné SessionEvent en retard, événements perdus");
                    continue;
                }
                std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
                std::task::Poll::Pending => std::task::Poll::Pending,
            };
        }
    }
}

/// Session détenue localement, avec le curseur nécessaire pour ne publier
/// que les deltas (voir [`SessionClient::diff_and_bump`]) plutôt que tout le
/// document à chaque mutation.
struct SessionEntry {
    session: YrsSession,
    /// Vecteur d'état au dernier envoi (diffusion locale ou réception d'un
    /// diff distant) — la prochaine publication n'envoie que ce qui a changé
    /// depuis.
    last_synced: StateVector,
}

impl SessionEntry {
    fn new(session: YrsSession) -> Self {
        let last_synced = session.state_vector();
        Self { session, last_synced }
    }
}

/// Pont entre le stockage local des sessions CRDT (voir
/// `session::crdt::YrsSession`) et le réseau : centralise la prise en charge
/// d'une session (création, ou synchronisation depuis un détenteur déjà
/// actif) et son maintien à jour en continu — une fois acquise, une session
/// reste synchronisée avec tous ses détenteurs actifs (potentiellement
/// plusieurs, si plusieurs frames de la même session tournent en parallèle
/// sur des workers différents) via [`SESSION_SYNC_TOPIC`], pas seulement au
/// moment de l'acquisition. Sert aussi les demandes
/// [`RpcCall::FETCH_SESSION`] d'un pair qui démarre.
///
/// Bon marché à cloner (comme [`NetworkClient`]) : pensé pour être threadé
/// dans les tâches de fond au même titre que lui, plutôt que de faire
/// transiter chaque accès par la boucle mono-thread de `NetworkActor`.
#[derive(Clone)]
pub struct SessionClient {
    network: NetworkClient,
    sessions: Arc<RwLock<HashMap<SessionId, SessionEntry>>>,
    events: broadcast::Sender<SessionEvent>,
    filesystem: SessionFilesystem,
}

impl SessionClient {
    /// S'abonne lui-même au flux d'événements réseau de `network` (voir
    /// `NetworkClient::subscribe_events`) et démarre sa propre tâche de fond
    /// pour traiter les messages gossipés sur [`SESSION_EVENTS_TOPIC`] (réémis
    /// aux abonnés locaux, voir [`Self::subscribe`]) et [`SESSION_SYNC_TOPIC`]
    /// (fusionnés dans les sessions détenues localement) — l'appelant n'a donc
    /// pas besoin de savoir filtrer ni forwarder quoi que ce soit lui-même.
    ///
    /// `filesystem` : contrairement au contenu CRDT (propre à ce worker tant
    /// qu'il n'est pas synchronisé), un [`SessionFilesystem`] est déjà un
    /// stockage partagé (voir `persistency::FilesystemConfig`) — le lire ou
    /// l'écrire ici ne nécessite donc pas d'avoir préalablement `acquire`
    /// la session.
    pub fn new(network: NetworkClient, filesystem: SessionFilesystem) -> Self {
        let (events, _) = broadcast::channel(SESSION_EVENTS_CAPACITY);
        network.subscribe(SESSION_EVENTS_TOPIC);
        network.subscribe(SESSION_SYNC_TOPIC);

        let sessions = Arc::new(RwLock::new(HashMap::new()));
        tokio::spawn(ingest_network_events(network.subscribe_events(), events.clone(), sessions.clone()));

        Self { network, sessions, events, filesystem }
    }

    /// S'abonne aux événements de cycle de vie des sessions — les siens
    /// comme ceux gossipés par d'autres pairs (voir [`SessionEvent`]).
    /// Chaque abonné reçoit sa propre copie ; les événements émis avant
    /// l'abonnement ne sont pas rejoués.
    pub fn subscribe(&self) -> SessionEventHandler {
        SessionEventHandler(BroadcastStream::new(self.events.subscribe()))
    }

    /// Diffuse `event` aux abonnés locaux (voir [`Self::subscribe`]) et au
    /// reste du cluster via gossipsub (voir [`SESSION_EVENTS_TOPIC`]) —
    /// best-effort dans les deux cas : ni l'absence d'abonné local, ni
    /// l'absence de pair dans le mesh gossipsub, ne fait échouer l'opération
    /// qui a produit l'événement.
    fn emit(&self, event: SessionEvent) {
        if let Err(error) = self.network.publish(SESSION_EVENTS_TOPIC, &event) {
            debug!(%error, ?event, "publication gossip de l'événement de session échouée");
        }
        let _ = self.events.send(event);
    }

    /// Diffuse `diff` aux autres détenteurs de `session_id` via
    /// [`SESSION_SYNC_TOPIC`] — best-effort, comme [`Self::emit`].
    fn publish_sync(&self, session_id: SessionId, diff: Vec<u8>) {
        let message = SessionSyncMessage { session_id, diff };
        if let Err(error) = self.network.publish(SESSION_SYNC_TOPIC, &message) {
            debug!(%error, %session_id, "publication du diff de session échouée");
        }
    }

    /// Prend en charge la session ciblée par le job en cours : la synchronise
    /// en interrogeant `known_holders` dans l'ordre jusqu'à ce que l'un
    /// réponde, ou en crée une vierge si la liste est vide (ce worker est le
    /// premier à exécuter un frame de cette session). Ne fait rien si elle
    /// est déjà détenue localement (réexécution sur ce même worker) — dans ce
    /// cas [`SessionEvent::Created`] n'est pas réémis.
    ///
    /// Une fois acquise, la session reste à jour en continu via
    /// [`SESSION_SYNC_TOPIC`] (voir [`ingest_network_events`]) : inutile de
    /// fusionner tous les `known_holders` ici, un seul suffit pour amorcer,
    /// les diffs des autres détenteurs actifs arriveront par ce flux.
    pub async fn acquire(&self, session_id: SessionId, known_holders: &[PeerId]) -> anyhow::Result<()> {
        if self.sessions.read().await.contains_key(&session_id) {
            return Ok(());
        }

        let session = if known_holders.is_empty() {
            YrsSession::new(session_id)
        } else {
            self.fetch_from_any(session_id, known_holders).await?
        };

        self.sessions.write().await.insert(session_id, SessionEntry::new(session));
        self.emit(SessionEvent::Created { session_id });
        Ok(())
    }

    /// Change le statut d'un frame connu de la session (voir
    /// [`crate::agent::status::AgentStatus`]), diffuse
    /// [`SessionEvent::FrameStatusChanged`] et publie le delta CRDT résultant
    /// (voir [`SESSION_SYNC_TOPIC`]).
    pub async fn set_frame_status(&self, session_id: SessionId, local_id: ID, status: AgentStatus) -> anyhow::Result<()> {
        let diff = {
            let mut sessions = self.sessions.write().await;
            let Some(entry) = sessions.get_mut(&session_id) else {
                bail!("session {session_id} inconnue de ce worker");
            };
            entry.session.set_status(local_id, &status)?;
            self.diff_and_bump(entry)
        };

        self.publish_sync(session_id, diff);
        self.emit(SessionEvent::FrameStatusChanged { session_id, local_id, status });
        Ok(())
    }

    /// Ajoute une entrée au journal de la session (voir [`SessionLog`]),
    /// diffuse [`SessionEvent::LogAppended`] et publie le delta CRDT résultant
    /// (voir [`SESSION_SYNC_TOPIC`]).
    pub async fn push_log(&self, session_id: SessionId, log: SessionLog) -> anyhow::Result<()> {
        let diff = {
            let mut sessions = self.sessions.write().await;
            let Some(entry) = sessions.get_mut(&session_id) else {
                bail!("session {session_id} inconnue de ce worker");
            };
            entry.session.push_log(&log)?;
            self.diff_and_bump(entry)
        };

        self.publish_sync(session_id, diff);
        self.emit(SessionEvent::LogAppended { session_id, log });
        Ok(())
    }

    /// Contenu du fichier `path` de la session, ou `None` s'il n'existe pas
    /// (voir [`SessionFilesystem::read`]).
    pub async fn read_file(&self, session_id: SessionId, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        self.filesystem.read(session_id, path).await
    }

    /// Écrit (ou remplace) le fichier `path` de la session (voir
    /// [`SessionFilesystem::write`]).
    pub async fn write_file(&self, session_id: SessionId, path: &str, data: Vec<u8>) -> anyhow::Result<()> {
        self.filesystem.write(session_id, path, data).await
    }

    /// Supprime le fichier `path` de la session (voir
    /// [`SessionFilesystem::delete`]).
    pub async fn delete_file(&self, session_id: SessionId, path: &str) -> anyhow::Result<()> {
        self.filesystem.delete(session_id, path).await
    }

    /// Chemins de tous les fichiers connus de la session (voir
    /// [`SessionFilesystem::list`]).
    pub async fn list_files(&self, session_id: SessionId) -> anyhow::Result<Vec<String>> {
        self.filesystem.list(session_id).await
    }

    /// Retire la session du stockage local de ce worker (par exemple une
    /// fois le job terminé) et diffuse [`SessionEvent::Removed`]. Ne fait
    /// rien si elle n'était pas détenue. Purement local : les autres
    /// détenteurs actifs, s'il y en a, conservent leur copie — les fichiers
    /// de la session, stockage partagé, ne sont pas concernés (voir
    /// `RpcCall::DELETE_SESSION` pour la suppression définitive).
    pub async fn remove(&self, session_id: SessionId) {
        if self.sessions.write().await.remove(&session_id).is_some() {
            self.emit(SessionEvent::Removed { session_id });
        }
    }

    /// Calcule le diff depuis le dernier envoi/réception et avance le
    /// curseur — à appeler juste après toute mutation locale, avant de
    /// relâcher le verrou d'écriture (voir [`SessionEntry::last_synced`]).
    fn diff_and_bump(&self, entry: &mut SessionEntry) -> Vec<u8> {
        let diff = entry.session.diff_since(&entry.last_synced);
        entry.last_synced = entry.session.state_vector();
        diff
    }

    /// Récupère l'état CRDT complet d'une session en interrogeant `holders`
    /// dans l'ordre jusqu'à ce que l'un réponde.
    async fn fetch_from_any(&self, session_id: SessionId, holders: &[PeerId]) -> anyhow::Result<YrsSession> {
        let mut last_error = None;

        for &holder in holders {
            match self.fetch_from(session_id, holder).await {
                Ok(session) => return Ok(session),
                Err(error) => {
                    debug!(%error, %session_id, %holder, "récupération de session échouée, essai du détenteur suivant");
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("aucun détenteur connu pour la session {session_id}")))
    }

    /// Récupère l'état CRDT complet d'une session auprès d'un détenteur
    /// connu (voir [`RpcCall::FETCH_SESSION`]) — on part d'un vecteur d'état
    /// vide : ce worker n'a par construction jamais vu cette session (sinon
    /// [`Self::acquire`] n'aurait pas appelé cette méthode).
    async fn fetch_from(&self, session_id: SessionId, holder: PeerId) -> anyhow::Result<YrsSession> {
        let request = SessionFetchRequest { session_id, state_vector: StateVector::default().encode_v1() };
        let diff: Vec<u8> = self.network.rpc_to(RpcCall::new(RpcCall::FETCH_SESSION, request), holder).await?;
        YrsSession::from_diff(&diff)
    }

    /// Répond à une demande [`RpcCall::FETCH_SESSION`] d'un pair : le diff
    /// depuis son vecteur d'état, si nous détenons encore la session.
    pub async fn serve_fetch(&self, request: SessionFetchRequest) -> anyhow::Result<Vec<u8>> {
        let remote_sv = StateVector::decode_v1(&request.state_vector).map_err(|error| anyhow::anyhow!(error))?;

        let sessions = self.sessions.read().await;
        let Some(entry) = sessions.get(&request.session_id) else {
            bail!("session {} inconnue de ce worker", request.session_id);
        };

        Ok(entry.session.diff_since(&remote_sv))
    }
}

/// Tâche de fond démarrée par [`SessionClient::new`] : consomme
/// `network_events` et traite les messages gossipés sur
/// [`SESSION_EVENTS_TOPIC`] (réémis sur `events`) et [`SESSION_SYNC_TOPIC`]
/// (diffs fusionnés dans `sessions`, s'ils concernent une session détenue
/// localement — ignorés sinon, voir la note sur la fenêtre de course
/// ci-dessous). Jamais re-gossipé (évite les boucles, à la manière de
/// `cp::RpcRegistryGossip`). Tout événement réseau qui n'est pas un
/// `GossipMessageReceived` sur l'un de ces deux topics est ignoré
/// silencieusement.
///
/// Fenêtre de course connue et acceptée : un diff reçu pendant qu'
/// [`SessionClient::acquire`] est en cours pour la même session (entre le
/// début du fetch et l'insertion dans `sessions`) est perdu, faute
/// d'endroit où le mettre en attente. Sans conséquence en pratique : le
/// fetch en cours récupère de toute façon l'état le plus récent connu du
/// détenteur interrogé, et les diffs suivants du même émetteur continueront
/// d'arriver normalement une fois la session insérée.
async fn ingest_network_events(
    mut network_events: NetworkEventHandler,
    events: broadcast::Sender<SessionEvent>,
    sessions: Arc<RwLock<HashMap<SessionId, SessionEntry>>>,
) {
    while let Some(event) = network_events.next().await {
        let NetworkEvent::GossipMessageReceived { topic, data, .. } = event else {
            continue;
        };

        if topic == SESSION_EVENTS_TOPIC {
            if let Ok(event) = serde_json::from_slice::<SessionEvent>(&data) {
                let _ = events.send(event);
            }
            continue;
        }

        if topic == SESSION_SYNC_TOPIC {
            let Ok(message) = serde_json::from_slice::<SessionSyncMessage>(&data) else {
                continue;
            };

            let mut sessions = sessions.write().await;
            let Some(entry) = sessions.get_mut(&message.session_id) else {
                continue;
            };

            if let Err(error) = entry.session.apply_diff(&message.diff) {
                debug!(%error, session_id = %message.session_id, "diff de session reçu illisible, ignoré");
                continue;
            }
            entry.last_synced = entry.session.state_vector();
        }
    }
}
