//! Catalogue de modèles répliqué entre plusieurs nœuds via [raft](openraft) (protocole de
//! consensus) transporté sur [libp2p].
//!
//! Chaque nœud fait tourner une instance [`openraft::Raft`] dont la machine à états est la
//! table `id -> `[`ModelDeclaration`]. Les mutations (`set`/`remove`) passent par
//! `Raft::client_write`, qui les journalise puis les réplique sur une majorité des nœuds avant
//! de les appliquer. Les lectures (`get`/`list`) contournent raft et lisent directement l'état
//! local, donc peuvent renvoyer une valeur légèrement en retard sur les autres nœuds.
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use libp2p::{Multiaddr, identity};
use openraft::error::{ClientWriteError, Fatal, InitializeError, RaftError};
use openraft::{Config, ConfigError, Raft};
use thiserror::Error;

use crate::model::declaration::{ModelDeclaration, ModelId};

mod network;
mod store;
mod types;

pub use types::{CatalogRequest, CatalogResponse, NodeId, RaftNode, TypeConfig};

use network::MarieNetworkFactory;
use store::{LogStore, StateMachineStore};

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("configuration raft invalide : {0}")]
    Config(#[from] ConfigError),
    #[error("échec du démarrage du réseau libp2p : {0}")]
    Network(String),
    #[error("échec de l'écriture distribuée : {0}")]
    Write(#[from] RaftError<NodeId, ClientWriteError<NodeId, RaftNode>>),
    #[error("échec de l'initialisation du cluster : {0}")]
    Initialize(#[from] RaftError<NodeId, InitializeError<NodeId, RaftNode>>),
    #[error("nœud raft arrêté de façon inattendue : {0}")]
    Fatal(#[from] Fatal<NodeId>)
}

/// Catalogue de modèles partagé entre plusieurs nœuds.
///
/// Se clone à moindre coût : toutes les instances clonées pilotent le même nœud raft.
#[derive(Clone)]
pub struct ModelCatalog {
    id: NodeId,
    local_node: RaftNode,
    raft: Raft<TypeConfig>,
    state_machine: StateMachineStore
}

impl ModelCatalog {
    /// Démarre un nœud raft du catalogue, en écoute sur `listen_addr` (ex :
    /// `/ip4/0.0.0.0/tcp/9000`).
    ///
    /// Le nœud ainsi créé est isolé : il faut soit appeler [`Self::init_cluster`] pour former un
    /// cluster à un seul nœud, soit le faire ajouter comme apprenti (learner) par un nœud membre
    /// d'un cluster existant via [`Self::add_learner`].
    pub async fn start(id: NodeId, listen_addr: Multiaddr, keypair: identity::Keypair) -> Result<Self, CatalogError> {
        let config = Arc::new(Config::default().validate()?);

        let log_store = LogStore::default();
        let state_machine = StateMachineStore::default();

        let (handle, local_addr, raft_tx) = network::spawn(keypair, listen_addr)
            .await
            .map_err(|err| CatalogError::Network(err.to_string()))?;

        let network_factory = MarieNetworkFactory::new(handle);

        let raft = Raft::new(id, config, network_factory, log_store, state_machine.clone()).await?;

        // Alimente la tâche réseau, qui attendait cette instance pour pouvoir répondre aux RPC
        // raft entrantes (voir la documentation de `network::spawn`).
        let _ = raft_tx.send(raft.clone());

        Ok(Self { id, local_node: RaftNode::new(local_addr.to_string()), raft, state_machine })
    }

    /// Identifiant raft de ce nœud.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Adresse libp2p complète (`/.../p2p/<peer_id>`) de ce nœud, à transmettre aux autres nœuds
    /// pour qu'ils puissent l'ajouter au cluster via [`Self::add_learner`].
    pub fn local_node(&self) -> &RaftNode {
        &self.local_node
    }

    /// Forme un cluster à un seul nœud, avec ce nœud comme unique voteur.
    ///
    /// Sans effet (autre que l'erreur ignorée par l'appelant) si le cluster est déjà formé : les
    /// autres nœuds doivent rejoindre via [`Self::add_learner`] puis [`Self::promote`].
    pub async fn init_cluster(&self) -> Result<(), CatalogError> {
        let members = BTreeMap::from([(self.id, self.local_node.clone())]);
        self.raft.initialize(members).await?;
        Ok(())
    }

    /// Ajoute `node` au cluster comme apprenti (réplique les logs mais ne vote pas).
    ///
    /// À appeler sur le nœud raft qui est actuellement leader.
    pub async fn add_learner(&self, id: NodeId, node: RaftNode) -> Result<(), CatalogError> {
        self.raft.add_learner(id, node, true).await?;
        Ok(())
    }

    /// Promeut les apprentis donnés en voteurs, en remplacement de la liste de voteurs actuelle.
    ///
    /// À appeler sur le nœud raft qui est actuellement leader, après [`Self::add_learner`].
    pub async fn promote(&self, voters: impl IntoIterator<Item = NodeId>) -> Result<(), CatalogError> {
        self.raft.change_membership(voters, true).await?;
        Ok(())
    }

    /// Lit la déclaration associée à `id` dans l'état local de ce nœud.
    ///
    /// Lecture non linéarisable : peut être en retard par rapport au leader d'un intervalle de
    /// réplication.
    pub async fn get(&self, id: &str) -> Option<ModelDeclaration> {
        self.state_machine.get(id).await
    }

    /// Liste l'ensemble des modèles connus de l'état local de ce nœud.
    pub async fn list(&self) -> HashMap<ModelId, ModelDeclaration> {
        self.state_machine.list().await
    }

    /// Enregistre ou remplace la déclaration d'un modèle, répliquée sur une majorité du cluster
    /// avant de retourner. Retourne l'ancienne déclaration si elle existait.
    pub async fn set(&self, id: impl Into<ModelId>, declaration: ModelDeclaration) -> Result<Option<ModelDeclaration>, CatalogError> {
        let request = CatalogRequest::Set { id: id.into(), declaration };
        let response = self.raft.client_write(request).await?;
        Ok(response.response().previous.clone())
    }

    /// Retire un modèle du catalogue, répliqué sur une majorité du cluster avant de retourner.
    /// Retourne l'ancienne déclaration si elle existait.
    pub async fn remove(&self, id: impl Into<ModelId>) -> Result<Option<ModelDeclaration>, CatalogError> {
        let request = CatalogRequest::Remove { id: id.into() };
        let response = self.raft.client_write(request).await?;
        Ok(response.response().previous.clone())
    }
}
