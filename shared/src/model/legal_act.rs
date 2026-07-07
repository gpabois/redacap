use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Statut du workflow de validation d'un projet d'acte légal (voir `Claude.md`
/// § Workflow de validation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegalActStatus {
    Redaction,
    Verification,
    Approbation,
    Finalise,
}

impl fmt::Display for LegalActStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match self {
            LegalActStatus::Redaction => "redaction",
            LegalActStatus::Verification => "verification",
            LegalActStatus::Approbation => "approbation",
            LegalActStatus::Finalise => "finalise",
        };
        f.write_str(repr)
    }
}

impl FromStr for LegalActStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "redaction" => Ok(LegalActStatus::Redaction),
            "verification" => Ok(LegalActStatus::Verification),
            "approbation" => Ok(LegalActStatus::Approbation),
            "finalise" => Ok(LegalActStatus::Finalise),
            other => Err(format!("statut de projet d'acte légal inconnu : {other}")),
        }
    }
}

/// Projet d'acte légal en cours de rédaction : titre, domaine technique
/// (`domain_id`, fixé une fois pour toutes à la création, voir
/// `crate::model::Domain`) et autorité pour le compte de laquelle il est pris.
///
/// Distinct de [`LegalActUpdate`]/[`LegalActSnapshot`] (journal CRDT du corps de
/// l'acte) : porte les métadonnées relationnelles nécessaires à l'en-tête
/// ODT/PDF et au suivi du workflow de validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalAct {
    pub id: ID,
    pub title: String,
    pub domain_id: ID,
    pub authority_id: ID,
    pub status: LegalActStatus,
    pub created_by: ID,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à la création d'un projet d'acte légal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateLegalAct {
    pub title: String,
    pub domain_id: ID,
    pub authority_id: ID,
    pub created_by: ID,
}

/// Mise à jour incrémentale Yrs journalisée pour un acte légal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalActUpdate {
    pub legal_act_id: ID,
    pub seq: i64,
    pub update: Vec<u8>,
    pub author_id: ID,
    pub created_at: DateTime<Utc>,
}

/// Dernier instantané consolidé d'un acte légal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalActSnapshot {
    pub legal_act_id: ID,
    pub snapshot: Vec<u8>,
    pub seq: i64,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à la journalisation d'une mise à jour Yrs incrémentale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateLegalActUpdate {
    pub legal_act_id: ID,
    pub seq: i64,
    pub update: Vec<u8>,
    pub author_id: ID,
}

/// Contenu consolidé à écrire lors d'une consolidation de snapshot.
///
/// Contrairement aux `*Changeset`, ceci n'est pas une modification partielle : `snapshot`
/// et `seq` forment une paire atomique (le second décrit exactement l'état encodé dans le
/// premier) et doivent toujours être remplacés ensemble, jamais l'un sans l'autre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalActSnapshotConsolidation {
    pub snapshot: Vec<u8>,
    pub seq: i64,
}

/// Mise à jour incrémentale Yrs journalisée pour les commentaires/notes de
/// travail (voir `legal_act::Review`) d'un acte légal. Pendant de
/// [`LegalActUpdate`] pour ce second document Yrs, persisté dans une table
/// séparée (`legal_act_review_updates`) : les deux CRDT (corps, commentaires)
/// évoluent indépendamment et ne partagent ni journal ni instantané.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalActReviewUpdate {
    pub legal_act_id: ID,
    pub seq: i64,
    pub update: Vec<u8>,
    pub author_id: ID,
    pub created_at: DateTime<Utc>,
}

/// Dernier instantané consolidé des commentaires/notes de travail d'un acte
/// légal. Pendant de [`LegalActSnapshot`] pour ce second document Yrs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalActReviewSnapshot {
    pub legal_act_id: ID,
    pub snapshot: Vec<u8>,
    pub seq: i64,
    pub updated_at: DateTime<Utc>,
}

/// Attributs nécessaires à la journalisation d'une mise à jour Yrs
/// incrémentale des commentaires/notes de travail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateLegalActReviewUpdate {
    pub legal_act_id: ID,
    pub seq: i64,
    pub update: Vec<u8>,
    pub author_id: ID,
}

/// Contenu consolidé à écrire lors d'une consolidation de snapshot des
/// commentaires/notes de travail. Voir [`LegalActSnapshotConsolidation`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalActReviewSnapshotConsolidation {
    pub snapshot: Vec<u8>,
    pub seq: i64,
}
