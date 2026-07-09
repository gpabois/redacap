//! Pont entre `app` (qui définit les pages/`ServerFunction`s modifiant les
//! métadonnées d'un projet, voir `app::pages::project_metadata`) et `server`
//! (qui possède le registre des salles de collaboration, voir
//! `server::editor::state::EditorRooms`) : `server` dépendant déjà de `app`,
//! ce dernier ne peut pas nommer directement le type qui diffuse aux pairs
//! connectés à une salle, sous peine de dépendance circulaire. Ce module,
//! commun aux deux crates, en fournit l'interface minimale
//! ([`RoomBroadcaster`]), injectée par contexte Leptos (voir `server::run`),
//! ainsi que le seul message diffusé pour l'instant ([`MetadataChangedEvent`]).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Diffuse un message déjà sérialisé à tous les clients websocket connectés
/// à une salle d'édition, sans que l'appelant ait à connaître le registre des
/// salles ni la forme de la connexion websocket elle-même.
pub trait RoomBroadcaster: Send + Sync {
    /// Diffuse `payload` (JSON déjà sérialisé, de la forme attendue par
    /// `ServerMessage` côté client, voir `app::protocol::ServerMessage`) à
    /// tous les pairs connectés à la salle `room_id` ; sans effet si la
    /// salle n'a actuellement aucune connexion active, ou n'existe pas (voir
    /// `tokio::sync::broadcast::Sender::send`).
    fn broadcast(&self, room_id: &str, payload: String);
}

/// Poignée de [`RoomBroadcaster`] à injecter par contexte Leptos (voir
/// `server::run`), consommée par les `ServerFunction`s de `app` qui doivent
/// notifier les pairs d'une salle sans passer par une connexion websocket
/// active (ex. `app::pages::project_metadata::set_project_metadata`).
pub type SharedRoomBroadcaster = Arc<dyn RoomBroadcaster>;

/// Nature du changement porté par [`MetadataChangedEvent`] : distingue une
/// création d'une mise à jour d'une clé existante, en comparant simplement
/// `created_at`/`updated_at` de la ligne retournée par
/// `storage::legal_act_metadata::upsert_metadata` (identiques à la création,
/// puisque tous deux valent le `now()` de l'unique instruction `INSERT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataChangeKind {
    Created,
    Updated,
    Deleted,
}

/// Écriture ou suppression d'une métadonnée d'un projet, diffusée en temps
/// réel à tous les pairs connectés à sa salle d'édition (voir
/// `server::editor::protocol::ServerMessage::MetadataChanged` /
/// `app::protocol::ServerMessage::MetadataChanged`), qu'elle vienne de
/// l'agent IA (`by_agent`, voir `agent::tools::metadata::WriteMetadataTool`)
/// ou d'un autre utilisateur du panneau « Paramètres » (voir
/// `app::pages::project_metadata::ProjectMetadataPanel`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataChangedEvent {
    pub key: String,
    pub kind: MetadataChangeKind,
    pub by_agent: bool,
    /// Utilisateur à l'origine du changement, `None` si `by_agent` (l'agent
    /// n'a pas de pastille de présence propre) : sert au panneau à retrouver
    /// sa pastille dans `RoomHandle::connected_users`.
    pub actor_id: Option<String>,
}

/// Nature du changement porté par [`DocumentsChangedEvent`] : un document
/// n'est jamais modifié en place une fois fourni, seulement ajouté ou retiré.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentChangeKind {
    Uploaded,
    Deleted,
}

/// Ajout ou suppression d'un document d'un projet (voir
/// `shared::model::LegalActDocument`), diffusé en temps réel à tous les
/// pairs connectés à sa salle d'édition (voir
/// `server::editor::protocol::ServerMessage::DocumentsChanged` /
/// `app::protocol::ServerMessage::DocumentsChanged`), qu'il vienne de l'agent
/// IA (`by_agent`, en réponse à l'outil `request_document`) ou d'un autre
/// utilisateur du panneau « Fichiers » (voir
/// `app::pages::project_documents::ProjectFilesPanel`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentsChangedEvent {
    pub file_name: String,
    pub kind: DocumentChangeKind,
    pub by_agent: bool,
    /// Utilisateur à l'origine du changement, `None` si `by_agent` (l'agent
    /// n'a pas de pastille de présence propre) : sert au panneau à retrouver
    /// sa pastille dans `RoomHandle::connected_users`.
    pub actor_id: Option<String>,
}
