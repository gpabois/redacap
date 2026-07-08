//! Modèles de données partagés entre `storage`, `server` et `frontend`.
//!
//! Ces types portent uniquement des données (pas de logique métier ni de dépendance
//! à `sqlx`/`leptos`) afin de pouvoir circuler tels quels entre les trois systèmes.

pub mod agent_profile;
pub mod agent_run;
pub mod agent_session;
pub mod agent_tool_scope;
pub mod ai_model;
pub mod audit_log;
pub mod authority;
pub mod configuration;
pub mod domain;
pub mod external_credentials;
pub mod group;
pub mod intention;
pub mod legal_act;
pub mod oidc_provider;
pub mod permission;
pub mod session;
pub mod user;

pub use agent_profile::*;
pub use agent_run::*;
pub use agent_session::*;
pub use agent_tool_scope::*;
pub use ai_model::*;
pub use audit_log::*;
pub use authority::*;
pub use configuration::*;
pub use domain::*;
pub use external_credentials::*;
pub use group::*;
pub use intention::*;
pub use legal_act::*;
pub use oidc_provider::*;
pub use permission::*;
pub use session::*;
pub use user::*;
