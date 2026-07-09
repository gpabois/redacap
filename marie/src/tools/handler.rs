use async_trait::async_trait;
use serde_json::Value;

use crate::tools::ToolCallScope;


pub enum ToolError {
    LostConnection,
    Panicked
}

#[async_trait]
pub trait ToolHandler {
    async fn call(&self, name: &str, params: Value, scope: ToolCallScope) -> Result<Value, ToolError>;
}