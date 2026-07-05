use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ID;

/// Session d'authentification, associée côté client à un cookie opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: ID,
    pub user_id: ID,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Attributs nécessaires à la création d'une session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateSession {
    pub user_id: ID,
    pub expires_at: DateTime<Utc>,
}
