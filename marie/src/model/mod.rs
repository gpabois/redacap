use async_openai::{Client, config::OpenAIConfig, error::OpenAIError, types::responses::{CreateResponseArgs, FunctionTool, Tool}};
use shared::id::ID;
use thiserror::Error;

use crate::{model::declaration::ModelDeclaration, tools::{ToolCall, ToolSignature}};

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
    UnknownModel(String)
}

pub struct ModelResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>
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
                    id : shared::id::generate_id(),
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