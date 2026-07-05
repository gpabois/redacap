use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Modèle de langage compatible avec l'API de complétion de chat OpenAI,
/// configurable depuis le panneau administrateur (`/admin/ai-models`) et
/// utilisable comme moteur de l'agent IA « Marie ». Au plus un modèle est
/// `active` à la fois (voir index `ai_models_single_active_idx`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiModel {
    pub id: ID,
    pub name: String,
    /// Racine de l'API, sans le segment `/chat/completions`.
    pub base_url: String,
    /// Identifiant du modèle transmis au fournisseur (ex: `gpt-4o-mini`).
    pub model: String,
    /// Clé API chiffrée au repos ; le déchiffrement est réservé à `server`.
    pub api_key_encrypted: Vec<u8>,
    /// Prompt système propre à ce modèle, ajouté en entête des contextes de
    /// domaine et d'intentions (voir `server::editor::ws::build_agent_context`).
    pub system_prompt: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à l'enregistrement d'un modèle IA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateAiModel {
    pub name: String,
    pub base_url: String,
    pub model: String,
    /// Clé API déjà chiffrée au repos ; le déchiffrement est réservé à `server`.
    pub api_key_encrypted: Vec<u8>,
    pub system_prompt: String,
}

/// Attributs modifiables d'un modèle IA existant.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés, les
/// champs à `None` conservent leur valeur actuelle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AiModelChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_encrypted: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}
