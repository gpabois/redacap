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
                "section_id": { "type": "string", "description": "Identifiant du noeud à remplir" },
                "content": { "type": "string", "description": "Contenu à insérer" }
            },
            "required": ["section_id", "content"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: FillSectionArguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments(error.to_string()))?;

        self.editor.fill_section(&args.section_id, &args.content).await?;
        Ok(ToolOutput::new("section mise à jour"))
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
            Ok(ToolOutput::new(format!("structure invalide : {}", report.errors.join("; "))))
        }
    }
}
