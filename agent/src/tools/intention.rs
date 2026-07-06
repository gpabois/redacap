//! Outils `list_intentions`, `add_intention` et `remove_intention`, qui
//! permettent à l'agent d'associer ou de retirer des intentions
//! rédactionnelles (ex. « mise en demeure ») du projet en cours d'édition,
//! sur demande explicite de l'inspecteur. Ces outils délèguent à
//! l'application hôte via [`IntentionPort`].

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::ToolError,
    ports::IntentionPort,
    tool::{Tool, ToolOutput},
};

/// Outil `list_intentions` : liste les intentions du domaine du projet en
/// cours, avec leur état d'association actuel — à appeler avant
/// `add_intention`/`remove_intention` pour résoudre le nom mentionné par
/// l'inspecteur en un identifiant technique, à la manière de
/// `read_structure` pour les nœuds de l'acte.
pub struct ListIntentionsTool {
    intentions: Arc<dyn IntentionPort>,
}

impl ListIntentionsTool {
    #[must_use]
    pub fn new(intentions: Arc<dyn IntentionPort>) -> Self {
        Self { intentions }
    }
}

#[async_trait]
impl Tool for ListIntentionsTool {
    fn name(&self) -> &str {
        "list_intentions"
    }

    fn description(&self) -> &str {
        "Liste les intentions du domaine du projet en cours (ex. « mise en demeure », « sanction \
         administrative »), avec leur état d'association actuel au projet."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        let intentions = self.intentions.list().await?;
        let value = serde_json::json!(
            intentions
                .into_iter()
                .map(|intention| serde_json::json!({
                    "id": intention.id,
                    "name": intention.name,
                    "attached": intention.attached,
                }))
                .collect::<Vec<_>>()
        );
        Ok(ToolOutput::new(value.to_string()))
    }
}

#[derive(Deserialize)]
struct IntentionArguments {
    intention_id: String,
}

/// Outil `add_intention` : associe une intention (identifiée via
/// `list_intentions`) au projet en cours d'édition, sur demande de
/// l'inspecteur.
pub struct AddIntentionTool {
    intentions: Arc<dyn IntentionPort>,
}

impl AddIntentionTool {
    #[must_use]
    pub fn new(intentions: Arc<dyn IntentionPort>) -> Self {
        Self { intentions }
    }
}

#[async_trait]
impl Tool for AddIntentionTool {
    fn name(&self) -> &str {
        "add_intention"
    }

    fn description(&self) -> &str {
        "Associe une intention (identifiant renvoyé par `list_intentions`) au projet en cours \
         d'édition."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "intention_id": {
                    "type": "string",
                    "description": "Identifiant de l'intention à associer, renvoyé par \
                        `list_intentions`"
                }
            },
            "required": ["intention_id"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: IntentionArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        self.intentions.add(&args.intention_id).await?;
        Ok(ToolOutput::new("intention associée au projet"))
    }
}

/// Outil `remove_intention` : retire une intention (identifiée via
/// `list_intentions`) du projet en cours d'édition, sur demande de
/// l'inspecteur.
pub struct RemoveIntentionTool {
    intentions: Arc<dyn IntentionPort>,
}

impl RemoveIntentionTool {
    #[must_use]
    pub fn new(intentions: Arc<dyn IntentionPort>) -> Self {
        Self { intentions }
    }
}

#[async_trait]
impl Tool for RemoveIntentionTool {
    fn name(&self) -> &str {
        "remove_intention"
    }

    fn description(&self) -> &str {
        "Retire une intention (identifiant renvoyé par `list_intentions`) du projet en cours \
         d'édition."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "intention_id": {
                    "type": "string",
                    "description": "Identifiant de l'intention à retirer, renvoyé par \
                        `list_intentions`"
                }
            },
            "required": ["intention_id"]
        })
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
        let args: IntentionArguments = serde_json::from_value(arguments)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        self.intentions.remove(&args.intention_id).await?;
        Ok(ToolOutput::new("intention retirée du projet"))
    }
}
