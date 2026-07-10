pub mod handler;
pub mod catalog;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::id::ID;

use crate::agent::GlobalAgentId;

pub type ToolName = String;

#[derive(Clone)]
pub struct ToolSignature {
    pub name: ToolName,
    pub description: String,
    pub parameters_schema: Value
}

pub enum ToolCallScope {
    Global,
    Session(ID)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: ID,
    pub name: ToolName,
    pub parameters: Option<Value>
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCallError {
    TimeOut,
    Custom(String)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    agent_id: GlobalAgentId,
    call: ToolCall
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCallResponse {
    Success {
        output: Option<Value>
    },
    Failed(ToolCallError),
}