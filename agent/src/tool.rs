use async_trait::async_trait;
use serde_json::Value;

use crate::{error::ToolError, model::ToolDefinition};

/// Résultat renvoyé par un outil après exécution, transmis au modèle comme
/// contenu du message `tool` correspondant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput(pub String);

impl ToolOutput {
    #[must_use]
    pub fn new(content: impl Into<String>) -> Self {
        Self(content.into())
    }
}

/// Une capacité exposée au modèle de langage pendant la boucle agentique
/// (ex: `legifrance_search`, `fill_section`...).
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    /// Schéma JSON des paramètres acceptés par l'outil.
    fn parameters_schema(&self) -> Value;

    /// Si `true`, l'agent doit obtenir une confirmation explicite de
    /// l'utilisateur avant d'exécuter l'outil — réservé aux actions
    /// irréversibles (remplacement de section, métadonnées critiques...).
    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError>;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}

/// Registre des outils disponibles pour une exécution de l'agent.
#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) -> &mut Self {
        self.tools.push(tool);
        self
    }

    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.definition()).collect()
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.iter().find(|tool| tool.name() == name).map(Box::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "renvoie ses arguments"
        }

        fn parameters_schema(&self) -> Value {
            json!({ "type": "object" })
        }

        async fn call(&self, arguments: Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::new(arguments.to_string()))
        }
    }

    #[test]
    fn registry_finds_registered_tool_by_name() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));

        assert!(registry.get("echo").is_some());
        assert!(registry.get("autre").is_none());
        assert_eq!(registry.definitions().len(), 1);
    }
}
