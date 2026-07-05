use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Domaine technique d'un acte légal (ex. « Installation classée »), géré par
/// les administrateurs. Fixé une fois pour toutes à la création d'un projet
/// (voir `LegalAct::domain_id`) et injecté comme contexte supplémentaire dans
/// le prompt système de l'agent IA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Domain {
    pub id: ID,
    pub name: String,
    pub agent_context: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à la création d'un domaine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateDomain {
    pub name: String,
    pub agent_context: String,
}

/// Attributs modifiables d'un domaine.
///
/// Chaque champ est optionnel : seuls les champs à `Some(_)` sont modifiés, les
/// champs à `None` conservent leur valeur actuelle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DomainChangeset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_context: Option<String>,
}
