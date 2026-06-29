//! Points d'intégration que l'application hôte (`server`) doit fournir pour
//! brancher l'agent sur l'état réel d'un projet en cours d'édition. Ce crate
//! ne dépend volontairement d'aucun type de `app`/`content` : les ports
//! utilisent des représentations opaques (`String`, [`serde_json::Value`])
//! pour rester découplés du modèle de domaine exact, à la manière des
//! handles opaques `ContentHandle`/`LegalActHandle`.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::error::ToolError;

/// Point d'intégration avec l'utilisateur courant (l'inspecteur), pour
/// l'outil `ask_user` et pour la confirmation des actions irréversibles.
#[async_trait]
pub trait UserInteractionPort: Send + Sync {
    /// Pose une question ouverte à l'utilisateur et renvoie sa réponse.
    async fn ask(&self, question: &str) -> Result<String, ToolError>;

    /// Demande une confirmation oui/non avant une action irréversible.
    async fn confirm(&self, message: &str) -> Result<bool, ToolError>;
}

/// Référence vers un document fourni par l'utilisateur en réponse à
/// `request_document`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentRef {
    pub id: String,
    pub file_name: String,
    pub mime_type: String,
}

/// Point d'intégration pour demander à l'utilisateur de fournir un document
/// externe (upload), pour l'outil `request_document`.
#[async_trait]
pub trait DocumentRequestPort: Send + Sync {
    async fn request_document(&self, prompt: &str, accepted_mime_types: &[String]) -> Result<DocumentRef, ToolError>;
}

/// Accès aux métadonnées contextuelles de l'acte en cours d'édition
/// (installation, rubriques ICPE, émissaires...), pour les outils
/// `read_metadata` et `write_metadata`.
#[async_trait]
pub trait MetadataPort: Send + Sync {
    async fn read(&self, key: &str) -> Result<Option<Value>, ToolError>;
    async fn write(&self, key: &str, value: Value) -> Result<(), ToolError>;
}

/// Rapport produit par `validate_structure`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub errors: Vec<String>,
}

impl ValidationReport {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Accès à la structure de l'acte en cours d'édition, pour les outils
/// `fill_section`, `generate_numbering` et `validate_structure`.
#[async_trait]
pub trait LegalActEditorPort: Send + Sync {
    /// Remplit ou complète le noeud identifié par `section_id` (article,
    /// considérant, visa...) avec `content`.
    async fn fill_section(&self, section_id: &str, content: &str) -> Result<(), ToolError>;

    /// Recalcule la numérotation de l'ensemble de l'acte.
    async fn generate_numbering(&self) -> Result<(), ToolError>;

    /// Vérifie les invariants structurels de l'acte.
    async fn validate_structure(&self) -> Result<ValidationReport, ToolError>;
}
