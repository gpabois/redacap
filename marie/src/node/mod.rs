use std::future::Future;
use std::sync::{Arc, OnceLock};

use thiserror::Error;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::error;
use typed_builder::TypedBuilder;

use crate::{
    model::{ModelClient, catalog::store::StoredModel},
    network::{
        actor::{NetworkActor, NetworkClient},
        cp, persistency as persistency_role,
        peer::NodeKind,
        start_swarm, worker,
    },
    persistency::{SessionFilesystem, SessionStore, store::Store},
    secret::{SecretKey, SecretManager},
    tools::{catalog::store::StoredTool, client::ToolClient},
};

/// Configuration d'un [`Marie`] : le secret maître du cluster (voir
/// [`SecretManager::new`]), à partager entre tous les nœuds destinés à
/// s'authentifier mutuellement. `master_key` doit être identique sur tous
/// les nœuds d'un même cluster — c'est ce secret, jamais l'identité libp2p
/// (régénérée à chaque démarrage, voir `network::start_swarm`), qui les
/// authentifie mutuellement.
#[derive(TypedBuilder)]
pub struct MarieConfig {
    master_key: SecretKey,
}

/// Rôle sous lequel un nœud rejoint le cluster (voir [`NodeKind`]) : chaque
/// variante correspond à une boucle de rôle existante (`network::cp`,
/// `network::worker`, `network::persistency`), démarrée par [`Marie::start`].
///
/// Un nœud tiers qui n'a besoin que de se brancher sur le réseau (sans
/// endosser de rôle de cluster) utilise [`Marie::join`] plutôt qu'une
/// variante de cette énumération.
pub enum NodeRole {
    /// `model_store` : stockage chiffré local du catalogue de modèles (voir
    /// `model::catalog::store` et `network::cp::start_control_plane`) —
    /// permet à ce nœud de récupérer son catalogue à froid sans dépendre du
    /// reste du cluster.
    ///
    /// `tool_store` : équivalent de `model_store` pour le catalogue de tools
    /// (voir `tools::catalog::store`).
    ControlPlane { model_store: Arc<dyn Store<StoredModel>>, tool_store: Arc<dyn Store<StoredTool>> },
    /// `filesystem` : fichiers accessibles aux sessions exécutées par ce
    /// worker (voir `network::worker::session_client::SessionClient::read_file`/
    /// `write_file`) — voir `persistency::FilesystemConfig` pour choisir le
    /// backend (mémoire, S3/compatible S3).
    Worker { filesystem: SessionFilesystem },
    /// `filesystem` : détenteur durable des fichiers de session, au même
    /// titre que `store` pour leur contenu CRDT — voir
    /// `network::persistency::start_persistency` et `RpcCall::DELETE_SESSION`.
    Persistency { store: Arc<dyn SessionStore>, filesystem: SessionFilesystem },
}

/// Poignée de supervision d'un nœud démarré par [`Marie`]. L'abandonner
/// n'arrête pas le nœud sous-jacent (voir [`tokio::task::JoinHandle`]) —
/// utiliser [`Self::abort`] pour l'arrêter explicitement, ou [`Self::wait`]
/// pour bloquer jusqu'à son arrêt (erreur de démarrage, ou panique).
pub struct MarieHandle {
    task: JoinHandle<()>,
}

impl MarieHandle {
    pub fn abort(&self) {
        self.task.abort();
    }

    pub async fn wait(self) {
        let _ = self.task.await;
    }
}

/// Point d'entrée unique pour configurer et démarrer un nœud du cluster Marie
/// (voir [`Self::start`]), ou pour simplement se brancher sur le réseau
/// depuis un nœud tiers développé par l'utilisateur (voir [`Self::join`]) —
/// par exemple une passerelle HTTP/WebSocket exposant du HITL, ou affichant
/// les logs/statuts d'une session (voir
/// `network::worker::session_client::SessionClient`, à construire par-dessus
/// le [`NetworkClient`] obtenu ici).
///
/// Tous les nœuds d'un même cluster doivent partager le même secret maître
/// (voir [`MarieConfig`]) : c'est lui, et non l'identité libp2p (régénérée à
/// chaque démarrage, voir `network::start_swarm`), qui permet
/// l'authentification mutuelle des control planes et le chiffrement des
/// secrets applicatifs transmis sur le réseau (voir
/// `NetworkClient::decrypt_secret`).
pub struct Marie {
    secret: Arc<SecretManager>,
    /// [`NetworkClient`] de ce nœud, rempli dès la connexion établie par
    /// [`Self::start`] ou [`Self::join`] — voir [`Self::model_client`]/
    /// [`Self::tool_client`]. `Arc` pour rester accessible depuis la tâche de
    /// fond qui le peuple (voir [`Self::start`]), indépendamment de la durée
    /// de vie d'un emprunt de `&self`.
    network: Arc<OnceLock<NetworkClient>>,
}

/// Retourné par [`Marie::model_client`]/[`Marie::tool_client`] tant que ce
/// nœud n'est pas encore connecté au réseau (voir [`Marie::start`]/
/// [`Marie::join`]) — la connexion est asynchrone, un appel juste après
/// [`Marie::start`] peut donc légitimement la précéder de peu.
#[derive(Debug, Error)]
#[error("nœud pas encore connecté au réseau (voir Marie::start / Marie::join)")]
pub struct NotConnected;

impl Marie {
    #[must_use]
    pub fn new(config: MarieConfig) -> Self {
        Self { secret: Arc::new(SecretManager::new(&config.master_key)), network: Arc::new(OnceLock::new()) }
    }

    /// Démarre un nœud endossant `role` en tâche de fond. La boucle de rôle
    /// tourne indéfiniment ; une erreur de démarrage (ex. port déjà occupé)
    /// est loggée puis met fin à la tâche, observable via
    /// [`MarieHandle::wait`].
    pub fn start(&self, role: NodeRole) -> MarieHandle {
        let secret = self.secret.clone();
        let (ready_tx, ready_rx) = oneshot::channel();
        let network = self.network.clone();
        tokio::spawn(async move {
            if let Ok(client) = ready_rx.await {
                let _ = network.set(client);
            }
        });

        let task = match role {
            NodeRole::ControlPlane { model_store, tool_store } => {
                Self::spawn_role("control-plane", cp::start_control_plane(secret, model_store, tool_store, ready_tx))
            }
            NodeRole::Worker { filesystem } => {
                Self::spawn_role("worker", worker::start_worker(secret, filesystem, ready_tx))
            }
            NodeRole::Persistency { store, filesystem } => {
                Self::spawn_role("persistency", persistency_role::start_persistency(secret, store, filesystem, ready_tx))
            }
        };

        MarieHandle { task }
    }

    /// Rejoint le réseau sans endosser de rôle de cluster (voir
    /// [`NodeKind::Client`]) : le point d'entrée pour un nœud développé par
    /// l'utilisateur qui a seulement besoin d'un [`NetworkClient`] pour
    /// émettre des RPC et observer les
    /// [`NetworkEvent`](crate::network::actor::NetworkEvent) du cluster (voir
    /// `NetworkClient::subscribe_events`), sans exécuter la logique d'un
    /// control plane, d'un worker ou d'un nœud de persistance.
    pub async fn join(&self) -> Result<(NetworkClient, MarieHandle), anyhow::Error> {
        let swarm = start_swarm(NodeKind::Client, |_| {}).await?;
        let (actor, client) = NetworkActor::new(swarm, self.secret.clone());
        let _ = self.network.set(client.clone());
        let task = tokio::spawn(actor.run());

        Ok((client, MarieHandle { task }))
    }

    /// Client pour le catalogue de modèles (voir [`ModelClient`]), une fois
    /// ce nœud connecté au réseau (voir [`Self::start`]/[`Self::join`]) —
    /// évite à l'appelant de conserver lui-même le [`NetworkClient`] obtenu à
    /// la connexion.
    pub fn model_client(&self) -> Result<ModelClient, NotConnected> {
        self.network.get().cloned().map(ModelClient::new).ok_or(NotConnected)
    }

    /// Client pour le catalogue de tools (voir [`ToolClient`]), sur le même
    /// modèle que [`Self::model_client`].
    pub fn tool_client(&self) -> Result<ToolClient, NotConnected> {
        self.network.get().cloned().map(ToolClient::new).ok_or(NotConnected)
    }

    fn spawn_role(
        name: &'static str,
        role: impl Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(error) = role.await {
                error!(%error, node = name, "nœud arrêté suite à une erreur");
            }
        })
    }
}
