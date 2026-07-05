//! Authentification : session opaque par cookie chiffré, connexion par
//! identifiants et par fournisseur OpenID Connect.
//!
//! Voir la contrainte racine « Authentification » (`Claude.md`) et
//! `server/CLAUDE.md` : cookie de session opaque (24h), authentification par
//! identifiants ou par l'un des fournisseurs OpenID Connect enregistrés.

pub mod bootstrap;
pub mod credentials;
pub mod crypto;
pub mod oidc;
pub mod session;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

/// Erreurs pouvant survenir lors de l'authentification.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("erreur de stockage : {0}")]
    Storage(#[from] storage::StorageError),
    #[error("erreur de chiffrement")]
    Crypto,
    #[error("erreur OIDC : {0}")]
    Oidc(String),
    #[error("identifiants invalides")]
    InvalidCredentials,
    #[error("fournisseur OIDC indisponible ou inconnu")]
    OidcUnavailable,
}

/// Monte les routes d'authentification sur l'état applicatif partagé.
pub fn build(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/login", post(credentials::login))
        .route("/logout", get(session::logout))
        .route("/bootstrap", post(bootstrap::create))
        .route("/oidc/{provider_id}/start", get(oidc::start))
        .route("/oidc/{provider_id}/callback", get(oidc::callback))
        .with_state(state)
}
