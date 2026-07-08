//! Persistance des données applicatives : ports/repositories typés sur Postgres.
//!
//! `storage` est le seul crate à connaître le schéma SQL. Il expose des fonctions de
//! repository par entité (`create_*`, `get_*`, `update_*`, `delete_*`, `list_*`) et ne
//! valide aucune règle métier ni permission : ceci reste la responsabilité de `server`.
//!
//! Les types de données échangés (`shared::model::*`) sont définis dans `shared` afin de
//! pouvoir circuler tels quels entre `storage`, `server` et `frontend` sans dépendre de
//! `sqlx`.

mod db;
mod error;
mod id;

pub mod agent_profile;
pub mod agent_run;
pub mod agent_tool_scope;
pub mod ai_model;
pub mod audit_log;
pub mod authority;
pub mod bootstrap;
pub mod configuration;
pub mod credential;
pub mod domain;
pub mod external_credentials;
pub mod group;
pub mod intention;
pub mod legal_act;
pub mod legal_act_review;
pub mod oidc_provider;
pub mod permission;
pub mod session;
pub mod user;
pub mod user_group;

pub use db::{Pool, connect, connect_lazy, migrate, revert};
pub use error::StorageError;
