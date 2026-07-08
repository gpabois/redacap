use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Session de conversation avec l'agent IA pour une salle d'édition (voir
/// migration `0017_agent_sessions`) : regroupe la chaîne de
/// [`crate::model::AgentRun`] démarrée à l'ouverture de la salle ou après
/// l'archivage de la précédente. Au plus une session `"active"` par salle
/// (voir `agent_sessions_active_per_room_idx`) ; les sessions `"archived"`
/// restent consultables en lecture seule (voir `server::editor::ws`,
/// `agent::AgentPanel`), sans jamais affecter la conversation en cours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: ID,
    pub room_id: String,
    /// Utilisateur ayant démarré cette session (premier message envoyé) :
    /// détermine à qui elle est proposée dans la liste des sessions passées
    /// (voir `storage::agent_session::list_sessions_for_room`).
    pub started_by: ID,
    /// `"active" | "archived"`.
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}
