pub mod handler;
pub mod catalog;

use serde_json::Value;
use shared::id::ID;

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

pub struct ToolCall {
    pub name: ToolName,
    pub parameters: Option<Value>
}

