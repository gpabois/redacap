use serde::{Deserialize, Serialize};
use crate::id::ID;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub enum AgentStatus {
    #[default]
    Initial,
    Paused,
    Running,
    Failed,
    Yielding(YieldStatus),
    Finished
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum YieldStatus {
    WaitingToolReply {
        tool_call_id: ID
    },
    RunExhausted
}