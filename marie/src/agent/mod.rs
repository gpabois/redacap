use crate::{agent::frame::AgentFrame, model::{self, ModelError, catalog::ModelCatalog}, tools::catalog::ToolCatalog};

pub mod status;
pub mod frame;
pub mod context;
pub mod role;


pub async fn run(
    frame: &mut AgentFrame,
    models: &ModelCatalog,
    tools: &ToolCatalog
) -> Result<model::ModelResponse, ModelError> {
    let signatures = frame
        .allowed_tools
        .iter()
        .flat_map(|tool_id| tools.get(tool_id))
        .map(|tool| tool.signature().clone())
        .collect::<Vec<_>>();

    let Some(model) = models.get(&frame.model_id).await else {
        return Err(ModelError::UnknownModel(frame.model_id.clone()));
    };

    model::execute(model, &signatures).await
}