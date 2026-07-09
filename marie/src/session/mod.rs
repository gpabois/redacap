use shared::id::ID;

use crate::{agent::frame::AgentFrame, tools::ToolCall};

pub struct Session {
    logs: SessionLog,
    frames: Vec<AgentFrame>
}

pub struct SessionLogBook(Vec<SessionLog>);

pub struct SessionLog {
    id: ID,
    data: SessionLogSpec
}

pub enum SessionLogSpec {
    AgentMessage {
        label: String,
        message: String
    },
    ToolCall(ToolCall)
}

