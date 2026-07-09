use std::{collections::HashMap, ops::Deref, sync::Arc};

use crate::tools::{ToolSignature, handler::ToolHandler};

pub struct ToolCatalog(HashMap<String, ToolDefinition>);

impl Deref for ToolCatalog {
    type Target = HashMap<String, ToolDefinition>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub enum ToolScope {
    Global,
    Session
}

#[derive(Clone)]
pub struct ToolDefinition {
    signature: ToolSignature,
    scope: ToolScope,
    handler: Arc<dyn ToolHandler>
}

impl ToolDefinition {
    pub fn signature(&self) -> &ToolSignature {
        &self.signature
    }
}