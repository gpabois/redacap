use std::io::Cursor;

use serde::{Deserialize, Serialize};

use crate::model::declaration::{ModelDeclaration, ModelId};

/// Identifiant d'un nœud raft du catalogue.
pub type NodeId = u64;

/// Informations réseau associées à un nœud raft (adresse libp2p complète, `/p2p/<peer_id>` inclus).
pub type RaftNode = openraft::BasicNode;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = CatalogRequest,
        R = CatalogResponse,
        NodeId = NodeId,
        Node = RaftNode,
);

/// Mutation appliquée à la machine à états du catalogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CatalogRequest {
    Set {
        id: ModelId,
        declaration: ModelDeclaration
    },
    Remove {
        id: ModelId
    }
}

/// Réponse renvoyée après application d'une [`CatalogRequest`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogResponse {
    /// Ancienne déclaration remplacée ou supprimée, si elle existait.
    pub previous: Option<ModelDeclaration>
}
