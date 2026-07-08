//! Outils de délégation dynamique à un ou plusieurs agents experts éphémères
//! (voir [`crate::catalog::AgentCatalog`]). Comme les outils d'interaction
//! (`agent::tools::interaction`), ils ne s'exécutent jamais eux-mêmes — voir
//! [`Tool::delegate_request`] — c'est l'orchestrateur qui empile un nouveau
//! frame éphémère pour la cible désignée :
//! - [`DelegateToExpertTool`] délègue à un profil nommé, choisi explicitement
//!   par l'appelant parmi le catalogue (schéma figé à la construction à
//!   partir des profils disponibles, jamais codé en dur) ;
//! - [`SpawnExpertTool`] délègue le choix de l'expert lui-même à une nouvelle
//!   instance du Superviseur (sous-tâche dynamique) : utile à un agent
//!   (Superviseur ou expert) qui identifie, en cours de tâche, un besoin dont
//!   il ne sait pas lui-même à quel profil du catalogue le confier — voir
//!   `agent::orchestration::AgentFrame::nested_supervisor`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    catalog::AgentProfile,
    error::ToolError,
    tool::{DelegateRequest, DelegateTarget, Tool, ToolOutput},
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
        "Délègue une sous-tâche précise à un agent expert éphémère nommé du catalogue, qui \
         dispose de son propre jeu d'outils et peut lui-même poser des questions à l'inspecteur \
         si besoin. Renvoie la réponse finale de l'expert une fois sa sous-tâche terminée."
    }

    fn parameters_schema(&self) -> Value {
        self.schema.clone()
    }

    fn delegate_request(&self, arguments: &Value) -> Result<Option<DelegateRequest>, ToolError> {
        let args: DelegateArguments = serde_json::from_value(arguments.clone())
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        Ok(Some(DelegateRequest {
            target: DelegateTarget::Profile(args.expert_id),
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

#[derive(Deserialize)]
struct SpawnExpertArguments {
    task: String,
}

/// Outil `spawn_expert` : sous-tâche dynamique confiée à une nouvelle
/// instance du Superviseur plutôt qu'à un expert nommé explicitement — voir
/// la documentation de module. Contrairement à [`DelegateToExpertTool`], son
/// schéma est statique (aucune dépendance au catalogue).
pub struct SpawnExpertTool;

#[async_trait]
impl Tool for SpawnExpertTool {
    fn name(&self) -> &str {
        "spawn_expert"
    }

    fn description(&self) -> &str {
        "Confie une sous-tâche autonome à une nouvelle instance du Superviseur, qui choisit \
         lui-même le ou les experts du catalogue les plus appropriés pour la mener à bien \
         (délégation simple ou en chaîne). À utiliser quand une sous-tâche identifiée en cours \
         de route dépasse ton propre périmètre et que tu ne sais pas toi-même à quel expert du \
         catalogue la confier, plutôt que d'essayer de la traiter toi-même. Renvoie la réponse \
         finale une fois la sous-tâche terminée."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Description précise et autonome de la sous-tâche à \
                                     confier (le Superviseur imbriqué ne voit pas la \
                                     conversation en cours, seulement ce texte)."
                }
            },
            "required": ["task"]
        })
    }

    fn delegate_request(&self, arguments: &Value) -> Result<Option<DelegateRequest>, ToolError> {
        let args: SpawnExpertArguments = serde_json::from_value(arguments.clone())
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        Ok(Some(DelegateRequest {
            target: DelegateTarget::Supervisor,
            task: args.task,
        }))
    }

    async fn call(&self, _arguments: Value) -> Result<ToolOutput, ToolError> {
        unreachable!(
            "spawn_expert est reconnu via Tool::delegate_request : l'orchestrateur ne doit \
             jamais appeler call() dessus"
        )
    }
}
