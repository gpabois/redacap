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

/// Une question posée à l'utilisateur dans le cadre d'un formulaire structuré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub id: String,
    pub label: String,
    /// Si `Some`, l'utilisateur doit choisir parmi ces options ;
    /// si `None`, il peut répondre librement par du texte.
    pub options: Option<Vec<String>>,
}

/// Réponse de l'utilisateur à une question du formulaire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionAnswer {
    pub question_id: String,
    pub value: String,
    /// Raison fournie par l'utilisateur si sa réponse n'est pas satisfaisante.
    pub unsatisfactory_reason: Option<String>,
}

/// Point d'intégration avec l'utilisateur courant (l'inspecteur), pour
/// l'outil `ask_user` et pour la confirmation des actions irréversibles.
#[async_trait]
pub trait UserInteractionPort: Send + Sync {
    /// Pose une question ouverte à l'utilisateur et renvoie sa réponse.
    async fn ask(&self, question: &str) -> Result<String, ToolError>;

    /// Demande une confirmation oui/non avant une action irréversible.
    async fn confirm(&self, message: &str) -> Result<bool, ToolError>;

    /// Présente un formulaire structuré à l'utilisateur et renvoie ses réponses.
    /// Chaque réponse peut indiquer qu'elle n'est pas satisfaisante via
    /// [`QuestionAnswer::unsatisfactory_reason`].
    async fn ask_questions(
        &self,
        prompt: &str,
        questions: &[Question],
    ) -> Result<Vec<QuestionAnswer>, ToolError>;
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
    async fn request_document(
        &self,
        prompt: &str,
        accepted_mime_types: &[String],
    ) -> Result<DocumentRef, ToolError>;
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
/// `read_structure`, `fill_section`, `insert_node`, `remove_node`,
/// `generate_numbering` et `validate_structure`.
#[async_trait]
pub trait LegalActEditorPort: Send + Sync {
    /// Lit l'arbre complet de l'acte : chaque noeud est représenté par un
    /// objet `{ id, kind, number?, text?, children? }` (`number` pour les
    /// noeuds numérotés, `text` pour les noeuds `Plain`, `children` pour les
    /// noeuds non-feuilles). Permet à l'agent de connaître le contenu
    /// existant sans jamais avoir à le demander à l'inspecteur.
    async fn read_structure(&self) -> Result<Value, ToolError>;

    /// Remplit ou complète le noeud identifié par `section_id` (article,
    /// considérant, visa...) avec `content`.
    async fn fill_section(&self, section_id: &str, content: &str) -> Result<(), ToolError>;

    /// Crée un nouveau noeud du type `kind` (ex. "Article", "Section",
    /// "Titre"...) sous le noeud `parent_id`, avec un contenu textuel
    /// initial optionnel, et renvoie l'identifiant du noeud créé.
    async fn insert_node(
        &self,
        parent_id: &str,
        kind: &str,
        content: Option<&str>,
    ) -> Result<String, ToolError>;

    /// Supprime le noeud `node_id` ainsi que tout son sous-arbre.
    async fn remove_node(&self, node_id: &str) -> Result<(), ToolError>;

    /// Recalcule la numérotation de l'ensemble de l'acte.
    async fn generate_numbering(&self) -> Result<(), ToolError>;

    /// Vérifie les invariants structurels de l'acte.
    async fn validate_structure(&self) -> Result<ValidationReport, ToolError>;

    /// Lit le titre de l'acte en cours d'édition (ex. « Arrêté préfectoral
    /// portant autorisation d'exploiter... »), distinct des noeuds `Titre`
    /// du corps (subdivisions numérotées « Titre I », « Titre II »...).
    /// Chaîne vide tant qu'aucun titre n'a été renseigné.
    async fn read_title(&self) -> Result<String, ToolError>;

    /// Définit ou remplace le titre de l'acte en cours d'édition.
    async fn set_title(&self, title: &str) -> Result<(), ToolError>;
}
