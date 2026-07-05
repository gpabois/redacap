use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Entrée du journal d'audit, immuable une fois écrite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: i64,
    pub occurred_at: DateTime<Utc>,
    pub actor_id: Option<ID>,
    pub actor_ip: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<ID>,
    pub details: Option<serde_json::Value>,
}

/// Attributs nécessaires pour tracer une action sensible dans le journal d'audit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateAuditEvent {
    pub actor_id: Option<ID>,
    pub actor_ip: Option<String>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<ID>,
    pub details: Option<serde_json::Value>,
}
