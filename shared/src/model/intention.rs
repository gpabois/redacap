use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Intention rédactionnelle d'un acte légal (ex. « mise en demeure »,
/// « sanction administrative »), rattachée à un [`crate::model::Domain`] :
/// seules les intentions du domaine d'un projet peuvent lui être associées.
/// Configurable par les administrateurs, ajoutable/supprimable directement
/// dans l'éditeur pour un projet donné, et injectée comme contexte
/// supplémentaire dans le prompt système de l'agent IA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Intention {
    pub id: ID,
    pub domain_id: ID,
    pub name: String,
    pub agent_context: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à la création d'une intention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateIntention {
    pub domain_id: ID,
    pub name: String,
    pub agent_context: String,
}

/// Attributs modifiables d'une intention.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés, les
/// champs à `None` conservent leur valeur actuelle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IntentionChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<ID>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_context: Option<String>,
}
