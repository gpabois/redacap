pub mod crdt;
pub mod sync;

use serde::{Deserialize, Serialize};
use crate::id::ID;

use crate::{agent::frame::AgentFrame, tools::ToolCall};

pub type SessionId = ID;

pub struct Session {
    logs: SessionLog,
    frames: Vec<AgentFrame>
}

pub struct SessionLogBook(Vec<SessionLog>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLog {
    id: ID,
    data: SessionLogSpec
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionLogSpec {
    AgentMessage {
        label: String,
        message: String
    },
    ToolCall(ToolCall)
}

