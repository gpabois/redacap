use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::ID;

/// Métadonnée contextuelle d'un projet d'acte légal (installation, rubriques
/// ICPE, émissaires...), en paire clé/valeur JSON libre : alimentée aussi
/// bien par l'inspecteur (panneau « Métadonnées » de l'éditeur, voir
/// `app::pages::project_metadata`) que par l'agent IA (outils
/// `read_metadata`/`write_metadata`/`search_metadata`, voir
/// `agent::tools::metadata`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalActMetadata {
    pub legal_act_id: ID,
    pub key: String,
    pub value: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
