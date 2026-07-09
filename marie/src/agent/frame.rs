use shared::id::ID;

use crate::agent::{context::Context, status::AgentStatus};

pub struct AgentFrame {
    /// The session id of the frame
    pub session_id: ID,
    /// The instance id
    pub id: ID,
    /// Model of the agent
    pub model_id: String,
    /// Current status of the agent
    pub status: AgentStatus,
    /// Allowed tools 
    pub allowed_tools: Vec<String>,
    /// Context
    pub context: Context
}
