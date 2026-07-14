use serde::{Deserialize, Serialize};
use crate::id::ID;

use crate::{
    agent::{context::Context, frame::AgentFrame},
    model::{self, ModelClient},
    network::worker::session_client::SessionClient,
    tools::{ToolCall, client::ToolClient},
};

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
    model: &ModelClient,
    tools: &ToolClient,
    sessions: &SessionClient,
) -> Result<model::ModelResponse, anyhow::Error> {
    let declaration = model.get(frame.model_id.clone()).await?;

    let mut signatures = Vec::with_capacity(frame.allowed_tools.len());
    for name in &frame.allowed_tools {
        signatures.push(tools.get(name.as_str()).await?.signature);
    }

    Ok(model::execute(declaration, &signatures).await?)
}
