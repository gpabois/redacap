use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Document externe fourni pour un projet d'acte légal (voir migration
/// `0020_legal_act_documents`) : soit uploadé directement par l'inspecteur
/// depuis le panneau « Fichiers » de l'éditeur (voir
/// `app::pages::project_documents`), soit fourni en réponse à une demande de
/// l'agent IA (outil `request_document`, voir `agent::tools::interaction`).
/// Rattaché au projet plutôt qu'à une session de conversation : il reste
/// disponible d'une session à l'autre, jusqu'à suppression explicite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegalActDocument {
    pub id: ID,
    pub legal_act_id: ID,
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
    /// Libellé sémantique du document (ex. « rapport d'inspection ICPE du
    /// 12/03/2024 »), distinct de `file_name` : permet de le retrouver par
    /// ce qu'il représente plutôt que par le nom de fichier brut, souvent
    /// opaque (`scan003.pdf`, voir migration `0021_legal_act_document_labels`).
    /// Chaîne vide si aucun libellé n'a été fourni.
    pub label: String,
    pub uploaded_by: ID,
    pub created_at: DateTime<Utc>,
}

/// Métadonnées d'un [`LegalActDocument`], sans son contenu binaire : pour
/// lister les documents d'un projet (panneau « Fichiers », outil
/// `search_documents`) sans transférer chaque fois l'intégralité des octets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalActDocumentSummary {
    pub id: ID,
    pub legal_act_id: ID,
    pub file_name: String,
    pub mime_type: String,
    /// Taille du contenu en octets (`octet_length(bytes)`).
    pub size: i64,
    /// Voir [`LegalActDocument::label`].
    pub label: String,
    pub uploaded_by: ID,
    pub created_at: DateTime<Utc>,
}
