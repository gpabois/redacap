//! Outil `delegate_to_expert` : point d'entrée unique et générique vers tout
//! le catalogue d'agents experts (voir [`crate::catalog::AgentCatalog`]).
//! Comme les outils d'interaction (`agent::tools::interaction`), il ne
//! s'exécute jamais lui-même — voir [`Tool::delegate_request`] — c'est
//! l'orchestrateur qui empile un nouveau frame éphémère pour le profil
//! désigné. Il n'existe qu'un seul type Rust ici, quel que soit le nombre de
//! profils enregistrés dans le catalogue : leur description (identifiant,
//! libellé) n'est utilisée que pour construire le schéma JSON exposé au
//! modèle, jamais codée en dur.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    catalog::AgentProfile,
    error::ToolError,
    tool::{DelegateRequest, Tool, ToolOutput},
};

#[derive(Deserialize)]
struct DelegateArguments {
    expert_id: String,
    task: String,
}

/// Outil `delegate_to_expert`, dont le schéma (`expert_id` en énumération)
/// est figé à la construction à partir des profils disponibles pour cette
/// connexion (voir `server::editor::ws::spawn_agent_run` /
/// `AgentCatalog::list`).
pub struct DelegateToExpertTool {
    schema: Value,
}

impl DelegateToExpertTool {
    #[must_use]
    pub fn new(profiles: &[AgentProfile]) -> Self {
        let expert_ids: Vec<&str> = profiles.iter().map(|profile| profile.id.as_str()).collect();
        let descriptions = profiles
            .iter()
            .map(|profile| format!("« {} » : {}", profile.id, profile.display_name))
            .collect::<Vec<_>>()
            .join(" ; ");
        Self {
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "expert_id": {
                        "type": "string",
                        "enum": expert_ids,
                        "description": format!(
                            "Identifiant de l'expert à qui déléguer la sous-tâche. Experts \
                             disponibles : {descriptions}"
                        )
                    },
                    "task": {
                        "type": "string",
                        "description": "Description précise et autonome de la sous-tâche \
                                         confiée à cet expert (il ne voit pas la conversation \
                                         en cours, seulement ce texte)"
                    }
                },
                "required": ["expert_id", "task"]
            }),
        }
    }
}

#[async_trait]
impl Tool for DelegateToExpertTool {
    fn name(&self) -> &str {
        "delegate_to_expert"
    }

    fn description(&self) -> &str {
        "Délègue une sous-tâche précise à un agent expert éphémère du catalogue, qui dispose de \
         son propre jeu d'outils et peut lui-même poser des questions à l'inspecteur si besoin. \
         Renvoie la réponse finale de l'expert une fois sa sous-tâche terminée."
    }

    fn parameters_schema(&self) -> Value {
        self.schema.clone()
    }

    fn delegate_request(&self, arguments: &Value) -> Result<Option<DelegateRequest>, ToolError> {
        let args: DelegateArguments = serde_json::from_value(arguments.clone())
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        Ok(Some(DelegateRequest {
            profile_id: args.expert_id,
            task: args.task,
        }))
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        unreachable!(
            "delegate_to_expert est reconnu via Tool::delegate_request : l'orchestrateur ne \
             doit jamais appeler call() dessus"
        )
    }
}
