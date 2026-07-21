//! Outils `fill_section`, `generate_numbering` et `validate_structure`, qui
//! délèguent à l'application hôte via [`LegalActEditorPort`].

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;


use crate::{
    error::ToolError,
    ports::LegalActEditorPort,
    tool::{Tool, ToolOutput},
};

/// Outil `read_structure` : lit l'arbre complet de l'acte en cours
/// d'édition (identifiants, types, numéros et contenu textuel de chaque
/// noeud). Outil de lecture seule, à utiliser avant toute opération qui
/// dépend du contenu existant plutôt que de demander à l'inspecteur de
/// copier-coller le texte des libellés ou articles déjà rédigés.
pub struct ReadStructureTool {
    editor: Arc<dyn LegalActEditorPort>,
}

impl ReadStructureTool {
    #[must_use]
    pub fn new(editor: Arc<dyn LegalActEditorPort>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Tool for ReadStructureTool {
    fn name(&self) -> &str {
        "read_structure"
    }

    fn description(&self) -> &str {
        "Lit l'arbre complet de l'acte en cours d'édition : pour chaque noeud (titre, chapitre, \
         article, annexe, visa, considérant, libellé, paragraphe...), son identifiant, son type, \
         son numéro le cas échéant et son contenu textuel. À appeler avant toute opération qui \
         dépend du contenu déjà rédigé (renumérotation, réécriture d'un libellé, détection d'un \
         doublon...) plutôt que de demander à l'inspecteur de copier-coller le texte existant."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        let structure = self.editor.read_structure().await?;
        Ok(ToolOutput::new(structure.to_string()))
    }
}

#[derive(Deserialize)]
struct FillSectionArguments {
    section_id: String,
    content: String,
}

/// Outil `fill_section` : remplit ou complète un noeud `LegalActContent`
/// (article, considérant, visa...). Remplacer le contenu d'une section
/// existante est une action irréversible : ce trait expose
/// [`Tool::requires_confirmation`] à `true` par défaut, l'agent doit donc
/// obtenir une confirmation de l'utilisateur avant exécution.
pub struct FillSectionTool {
    editor: Arc<dyn LegalActEditorPort>,
}

impl FillSectionTool {
    #[must_use]
    pub fn new(editor: Arc<dyn LegalActEditorPort>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Tool for FillSectionTool {
    fn name(&self) -> &str {
        "fill_section"
    }

    fn description(&self) -> &str {
        "Remplit ou complète un noeud de l'acte (article, considérant, visa...) avec le contenu fourni."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "section_id": {
                    "type": "string",
                    "description": "Identifiant du noeud à remplir : soit un identifiant technique \
                        renvoyé par un appel précédent à insert_node, soit le mot-clé « selection » \
                        pour viser le noeud actuellement ciblé par l'utilisateur dans l'éditeur \
                        (bouton « Cibler »). N'invente jamais un identifiant et ne demande jamais à \
                        l'utilisateur de t'en fournir un."
                },
                "content": { "type": "string", "description": "Contenu à insérer" }
            },
            "required": ["section_id", "content"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: FillSectionArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        self.editor
            .fill_section(&args.section_id, &args.content)
            .await?;
        Ok(ToolOutput::new("section mise à jour"))
    }
}

#[derive(Deserialize)]
struct InsertNodeArguments {
    parent_id: String,
    kind: String,
    #[serde(default)]
    content: Option<String>,
}

/// Outil `insert_node` : crée un nouveau noeud de la structure de l'acte
/// (article, section, titre, chapitre, annexe, visa, considérant,
/// paragraphe, tableau, liste...) sous un noeud parent existant, avec un
/// contenu textuel initial optionnel. Créer un noeud modifie la structure
/// de l'acte : ce trait expose [`Tool::requires_confirmation`] à `true`,
/// l'agent doit donc obtenir une confirmation de l'utilisateur avant
/// exécution.
pub struct InsertNodeTool {
    editor: Arc<dyn LegalActEditorPort>,
}

impl InsertNodeTool {
    #[must_use]
    pub fn new(editor: Arc<dyn LegalActEditorPort>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Tool for InsertNodeTool {
    fn name(&self) -> &str {
        "insert_node"
    }

    fn description(&self) -> &str {
        "Crée un nouveau noeud de la structure de l'acte (article, section, titre, chapitre, \
         annexe, visa, considérant, sur, paragraphe, tableau, liste...) sous un noeud parent \
         existant, avec un contenu textuel initial optionnel."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "parent_id": {
                    "type": "string",
                    "description": "Identifiant du noeud parent sous lequel insérer le nouveau noeud : \
                        soit un identifiant technique renvoyé par un appel précédent à insert_node, \
                        soit le mot-clé « root » pour insérer directement à la racine de l'acte (ex. \
                        un premier visa, considérant ou article), soit le mot-clé « selection » pour \
                        insérer sous le noeud actuellement ciblé par l'utilisateur dans l'éditeur \
                        (bouton « Cibler »). N'invente jamais un identifiant et ne demande jamais à \
                        l'utilisateur de t'en fournir un."
                },
                "kind": {
                    "type": "string",
                    "description": "Type du noeud à créer",
                    "enum": [
                        "Titre", "Section", "Chapitre", "Article", "Annexe",
                        "Visa", "Considerant", "Sur", "Paragraphe", "Table", "List"
                    ]
                },
                "content": { "type": "string", "description": "Contenu textuel initial du noeud créé (libellé ou premier paragraphe), optionnel" }
            },
            "required": ["parent_id", "kind"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: InsertNodeArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        let node_id = self
            .editor
            .insert_node(&args.parent_id, &args.kind, args.content.as_deref())
            .await?;
        Ok(ToolOutput::new(format!("noeud créé : {node_id}")))
    }
}

#[derive(Deserialize)]
struct RemoveNodeArguments {
    node_id: String,
}

/// Outil `remove_node` : supprime un noeud de la structure de l'acte, ainsi
/// que tout son sous-arbre. Action irréversible : ce trait expose
/// [`Tool::requires_confirmation`] à `true`, l'agent doit donc obtenir une
/// confirmation de l'utilisateur avant exécution.
pub struct RemoveNodeTool {
    editor: Arc<dyn LegalActEditorPort>,
}

impl RemoveNodeTool {
    #[must_use]
    pub fn new(editor: Arc<dyn LegalActEditorPort>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Tool for RemoveNodeTool {
    fn name(&self) -> &str {
        "remove_node"
    }

    fn description(&self) -> &str {
        "Supprime un noeud de la structure de l'acte (article, section, titre, chapitre, \
         annexe, paragraphe...) ainsi que tout son sous-arbre."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "node_id": {
                    "type": "string",
                    "description": "Identifiant du noeud à supprimer : soit un identifiant technique \
                        renvoyé par un appel précédent à insert_node, soit le mot-clé « selection » \
                        pour viser le noeud actuellement ciblé par l'utilisateur dans l'éditeur \
                        (bouton « Cibler »). N'invente jamais un identifiant et ne demande jamais à \
                        l'utilisateur de t'en fournir un."
                }
            },
            "required": ["node_id"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: RemoveNodeArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        self.editor.remove_node(&args.node_id).await?;
        Ok(ToolOutput::new("noeud supprimé"))
    }
}

/// Outil `generate_numbering` : recalcule la numérotation des noeuds après
/// une modification structurelle.
pub struct GenerateNumberingTool {
    editor: Arc<dyn LegalActEditorPort>,
}

impl GenerateNumberingTool {
    #[must_use]
    pub fn new(editor: Arc<dyn LegalActEditorPort>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Tool for GenerateNumberingTool {
    fn name(&self) -> &str {
        "generate_numbering"
    }

    fn description(&self) -> &str {
        "Recalcule la numérotation de l'ensemble des noeuds de l'acte après une modification structurelle."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        self.editor.generate_numbering().await?;
        Ok(ToolOutput::new("numérotation recalculée"))
    }
}

/// Outil `validate_structure` : vérifie que l'acte respecte les invariants
/// structurels avant génération.
pub struct ValidateStructureTool {
    editor: Arc<dyn LegalActEditorPort>,
}

impl ValidateStructureTool {
    #[must_use]
    pub fn new(editor: Arc<dyn LegalActEditorPort>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Tool for ValidateStructureTool {
    fn name(&self) -> &str {
        "validate_structure"
    }

    fn description(&self) -> &str {
        "Vérifie que l'acte respecte les invariants structurels (ex: position des annexes) avant génération."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        let report = self.editor.validate_structure().await?;

        if report.is_valid() {
            Ok(ToolOutput::new("structure valide"))
        } else {
            Ok(ToolOutput::new(format!(
                "structure invalide : {}",
                report.errors.join("; ")
            )))
        }
    }
}

/// Outil `read_title` : lit le titre de l'acte en cours d'édition (ex.
/// « Arrêté préfectoral portant autorisation d'exploiter... »), distinct des
/// noeuds `Titre` du corps (subdivisions numérotées « Titre I », « Titre
/// II »...).
pub struct ReadTitleTool {
    editor: Arc<dyn LegalActEditorPort>,
}

impl ReadTitleTool {
    #[must_use]
    pub fn new(editor: Arc<dyn LegalActEditorPort>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Tool for ReadTitleTool {
    fn name(&self) -> &str {
        "read_title"
    }

    fn description(&self) -> &str {
        "Lit le titre de l'acte en cours d'édition (ex. « Arrêté préfectoral portant autorisation \
         d'exploiter... »), distinct des noeuds Titre du corps (subdivisions numérotées « Titre I », \
         « Titre II »...). Chaîne vide tant qu'aucun titre n'a été renseigné."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        let title = self.editor.read_title().await?;
        Ok(ToolOutput::new(title))
    }
}

#[derive(Deserialize)]
struct SetTitleArguments {
    title: String,
}

/// Outil `set_title` : définit ou remplace le titre de l'acte en cours
/// d'édition. Remplacer le titre est une action irréversible : ce trait
/// expose [`Tool::requires_confirmation`] à `true` par défaut, l'agent doit
/// donc obtenir une confirmation de l'utilisateur avant exécution.
pub struct SetTitleTool {
    editor: Arc<dyn LegalActEditorPort>,
}

impl SetTitleTool {
    #[must_use]
    pub fn new(editor: Arc<dyn LegalActEditorPort>) -> Self {
        Self { editor }
    }
}

#[async_trait]
impl Tool for SetTitleTool {
    fn name(&self) -> &str {
        "set_title"
    }

    fn description(&self) -> &str {
        "Définit ou remplace le titre de l'acte en cours d'édition (ex. « Arrêté préfectoral portant \
         autorisation d'exploiter... »)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Nouveau titre de l'acte" }
            },
            "required": ["title"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: SetTitleArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        self.editor.set_title(&args.title).await?;
        Ok(ToolOutput::new("titre mis à jour"))
    }
}
