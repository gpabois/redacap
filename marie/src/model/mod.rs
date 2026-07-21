use std::collections::HashMap;

use async_openai::{Client, config::OpenAIConfig, error::OpenAIError, types::responses::{CreateResponseArgs, FunctionTool, Tool}};
use crate::id::ID;
use thiserror::Error;

use crate::{model::{catalog::ModelId, declaration::ModelDeclaration}, network::actor::NetworkClient, tools::{ToolCall, ToolSignature}};

pub mod catalog;
pub mod declaration;

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("échec de la requête : {0}")]
    OpenAIError(#[from] OpenAIError),
    #[error("échec lors de la réponse: {message} (code: {code})")]
    ResponseError {
        code: String,
        message: String
    },
    #[error("modèle inconnu : {0}")]
    UnknownModel(ModelId),
    #[error("échec de récupération du modèle : {0}")]
    Network(String),
}

pub struct ModelResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>
}

pub struct ModelClient(NetworkClient);

impl ModelClient {
    #[must_use]
    pub fn new(client: NetworkClient) -> Self {
        Self(client)
    }

    /// Récupère la déclaration d'un modèle auprès du control plane. La clé
    /// API a voyagé chiffrée sur le réseau — voir
    /// [`NetworkClient::get_model`] et `SecretManager` — et n'est déchiffrée
    /// en clair qu'à la réception, localement.
    pub async fn get(&self, id: impl Into<ModelId>) -> Result<ModelDeclaration, ModelError> {
        let id = id.into();

        self.0
            .get_model(id.clone())
            .await
            .map_err(|error| ModelError::Network(error.to_string()))?
            .ok_or(ModelError::UnknownModel(id))
    }

    /// Liste tout le catalogue de modèles connu du control plane.
    pub async fn list(&self) -> Result<HashMap<ModelId, ModelDeclaration>, ModelError> {
        self.0.list_models().await.map_err(|error| ModelError::Network(error.to_string()))
    }

    /// Crée ou remplace la déclaration d'un modèle dans le catalogue.
    pub async fn set(&self, id: impl Into<ModelId>, declaration: ModelDeclaration) -> Result<(), ModelError> {
        self.0.set_model(id, declaration).await.map_err(|error| ModelError::Network(error.to_string()))
    }

    /// Retire un modèle du catalogue.
    pub async fn remove(&self, id: impl Into<ModelId>) -> Result<(), ModelError> {
        self.0.remove_model(id).await.map_err(|error| ModelError::Network(error.to_string()))
    }
}


pub async fn execute(decl: ModelDeclaration, tools: &[ToolSignature]) -> Result<ModelResponse, ModelError> {
    let config = OpenAIConfig::new()
        .with_api_base(decl.base_url)
        .with_api_key(decl.api_key)
        .with_org_id(decl.client_id);

    let client = Client::with_config(config);

    let request = CreateResponseArgs::default()
        .model(decl.model)
        .tools(tools.iter().cloned().map(|sig| Tool::Function(FunctionTool {
            name: sig.name,
            description: Some(sig.description),
            parameters: Some(sig.parameters_schema),
            ..Default::default()

        })).collect::<Vec<_>>())
        .build()?;

    
    let response = client.responses().create(request).await?;

    if let Some(err) = response.error {
        return Err(ModelError::ResponseError { code: err.code, message: err.message })
    }

    let text = response.output_text();

    let tool_calls = response.tools.into_iter().flatten().flat_map(|tool| {
        match tool {
            Tool::Function(function_tool) => {
                Some(ToolCall {
                    id : crate::id::generate_id(),
                    name: function_tool.name,
                    parameters: function_tool.parameters
                })
            },
            _ => None
        }
    });

    Ok(ModelResponse {
        text,
        tool_calls: tool_calls.collect()
    })

}