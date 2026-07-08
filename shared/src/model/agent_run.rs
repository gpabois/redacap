use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::ID;

/// État persisté d'une orchestration en cours pour une salle d'édition (voir
/// `agent::orchestration::OrchestrationRun`, dont `stack` est la
/// sérialisation JSON de `Vec<agent::orchestration::AgentFrame>`) : c'est ce
/// qui permet à une pause (question posée à l'inspecteur, confirmation
/// requise...) de survivre à une déconnexion ou un redémarrage du serveur.
/// Ce crate ignore volontairement la structure interne de `stack` — seul
/// `server` (qui dépend à la fois de `storage` et d'`agent`) sait
/// l'interpréter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: ID,
    /// Salle d'édition à laquelle ce run appartient (voir
    /// `server::editor::state::EditorRoom`) ; au plus un run `running`/
    /// `paused` par salle (voir `agent_runs_active_per_room_idx`).
    pub room_id: String,
    /// Session de conversation à laquelle ce run appartient (voir
    /// [`crate::model::AgentSession`]) : plusieurs runs successifs
    /// (`resume_as_new_task`) partagent la même session tant qu'elle n'a pas
    /// été archivée.
    pub session_id: ID,
    pub author_id: ID,
    /// `"running" | "paused" | "done" | "failed"` (voir
    /// `agent::orchestration::RunStatus`).
    pub status: String,
    pub stack: Value,
    pub final_answer: Option<String>,
    /// Verrou optimiste : [`crate::model::AgentRunChangeset`] n'est appliqué
    /// que si `version` correspond encore à la valeur lue, pour détecter une
    /// modification concurrente plutôt que d'écraser silencieusement l'état
    /// d'une orchestration en cours.
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Document uploadé par l'inspecteur en réponse à une pause de type
/// `request_document`, persisté indépendamment de l'état en mémoire d'une
/// connexion websocket pour que la reprise fonctionne même sur une connexion
/// différente de celle où la demande a été faite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunDocument {
    pub id: ID,
    pub run_id: ID,
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub created_at: DateTime<Utc>,
}
