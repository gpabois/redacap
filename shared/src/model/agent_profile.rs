use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Configuration d'un agent expert éphémère du catalogue d'orchestration,
/// éditable depuis le panneau administrateur (`/admin/agent-profiles`) : au
/// lieu d'une struct Rust dédiée par expert (Visas, Motifs...), chaque
/// expert n'est que cette donnée, résolue par `name` au moment où le
/// Superviseur délègue une sous-tâche (voir `agent::catalog::AgentCatalog`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: ID,
    /// Identifiant technique stable, transmis au modèle comme valeur du
    /// paramètre `expert_id` de l'outil `delegate_to_expert` — jamais
    /// affiché tel quel (voir `display_name`).
    pub name: String,
    pub display_name: String,
    pub system_prompt: String,
    /// Sous-ensemble des outils disponibles pour cet expert (voir
    /// `agent::tool::ToolRegistry::subset`).
    pub tool_names: Vec<String>,
    pub max_steps: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à l'enregistrement d'un profil d'agent expert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateAgentProfile {
    pub name: String,
    pub display_name: String,
    pub system_prompt: String,
    pub tool_names: Vec<String>,
    pub max_steps: i32,
}

/// Attributs modifiables d'un profil d'agent expert existant.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés, les
/// champs à `None` conservent leur valeur actuelle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentProfileChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_names: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}
