use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::ToolError, model::ToolDefinition, ports::Question};

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

/// Requête d'intervention humaine renvoyée par [`Tool::pause_request`] :
/// contrairement à un outil normal, un outil qui en renvoie une (`ask_user`,
/// `ask_questions`, `request_document`) n'est **jamais** exécuté via
/// [`Tool::call`] — reconnu par ce hook avant tout appel, l'orchestrateur
/// suspend l'exécution du frame courant plutôt que d'attendre une réponse en
/// bloquant (voir `crate::orchestration`), pour que la pause puisse être
/// persistée et reprise plus tard, y compris après un redémarrage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PauseRequest {
    Ask {
        question: String,
    },
    AskQuestions {
        prompt: String,
        questions: Vec<Question>,
    },
    Confirm {
        message: String,
    },
    RequestDocument {
        prompt: String,
        accepted_mime_types: Vec<String>,
    },
}

/// Requête de délégation à un agent expert renvoyée par
/// [`Tool::delegate_request`] (voir `agent::tools::DelegateToExpertTool`,
/// `agent::tools::SpawnExpertTool`) : comme pour [`PauseRequest`], un outil
/// qui en renvoie une n'est jamais exécuté via [`Tool::call`] —
/// l'orchestrateur empile un nouveau frame éphémère pour la cible désignée
/// plutôt que d'exécuter l'outil lui-même.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelegateRequest {
    pub target: DelegateTarget,
    pub task: String,
}

/// Cible d'une [`DelegateRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DelegateTarget {
    /// Profil nommé du catalogue, choisi explicitement par l'appelant
    /// (`delegate_to_expert`).
    Profile(String),
    /// Nouvelle instance du Superviseur, qui choisit lui-même l'expert
    /// approprié plutôt que de le laisser à la charge de l'appelant
    /// (`spawn_expert`, sous-tâche dynamique) — voir
    /// `agent::orchestration::AgentFrame::nested_supervisor`.
    Supervisor,
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
    /// L'orchestrateur traduit ceci en [`PauseRequest::Confirm`] plutôt que
    /// d'attendre une confirmation en bloquant.
    fn requires_confirmation(&self) -> bool {
        false
    }

    /// `Some` si cet appel doit suspendre l'exécution en attente d'une
    /// réponse humaine plutôt que d'être exécuté normalement (voir
    /// [`PauseRequest`]). Défaut neutre : la quasi-totalité des outils
    /// n'ont rien à changer.
    fn pause_request(&self, _arguments: &Value) -> Result<Option<PauseRequest>, ToolError> {
        Ok(None)
    }

    /// `Some` si cet appel doit déléguer à un agent expert éphémère plutôt
    /// que d'être exécuté normalement (voir [`DelegateRequest`]). Défaut
    /// neutre : seul `delegate_to_expert` le redéfinit.
    fn delegate_request(&self, _arguments: &Value) -> Result<Option<DelegateRequest>, ToolError> {
        Ok(None)
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

/// Registre des outils disponibles pour une exécution de l'agent. Les outils
/// sont partagés (`Arc`) plutôt que possédés (`Box`) : un frame expert
/// éphémère n'a besoin que d'un sous-ensemble du registre complet construit
/// une fois par connexion (voir [`Self::subset`]), sans dupliquer leur
/// construction ni leurs éventuelles connexions réseau sous-jacentes.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) -> &mut Self {
        self.tools.push(Arc::from(tool));
        self
    }

    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|tool| tool.definition()).collect()
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|tool| tool.name() == name)
            .map(Arc::as_ref)
    }

    /// Construit un registre restreint aux outils dont le nom figure dans
    /// `names` (voir `agent::catalog::AgentProfile::tool_names`), pour
    /// délimiter les capacités d'un agent expert éphémère à celles définies
    /// par son profil. Un nom sans outil correspondant est silencieusement
    /// ignoré : mieux vaut un expert avec un outil en moins qu'un échec de
    /// démarrage pour toute la délégation à cause d'un profil mal renseigné
    /// dans le catalogue.
    #[must_use]
    pub fn subset(&self, names: &[String]) -> ToolRegistry {
        ToolRegistry {
            tools: self
                .tools
                .iter()
                .filter(|tool| names.iter().any(|name| name == tool.name()))
                .cloned()
                .collect(),
        }
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
