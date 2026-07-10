use serde::{Deserialize, Serialize};
use shared::id::ID;

use crate::{agent::{context::Context, frame::AgentFrame}, model::{self, ModelError, catalog::ModelCatalog}, tools::{ToolCall, ToolName, catalog::ToolCatalog}};

pub mod status;
pub mod frame;
pub mod context;
pub mod role;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalAgentId(ID, ID);

impl GlobalAgentId {
    pub fn session_id(&self) -> ID {
        self.0
    }

    pub fn local_id(&self) -> ID {
        self.1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Warmup operations executed juste after spawn and before running.
pub enum AgentWarmup {
    WriteContext(Context),
    ExecuteTool(ToolCall)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpawnRequest {
    session_id: ID,
    agent_id: ID,
    warmup: Vec<AgentWarmup>,
}

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
