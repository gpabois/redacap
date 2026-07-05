//! Garde l'état bootstrap (voir `Claude.md` § « Ajoute un état bootstrap... »).
//!
//! Tant qu'`AppState::bootstrap_required` est à `true`, toute requête autre
//! que `GET/POST /bootstrap` ou le chargement des ressources statiques
//! (`/pkg/...`, nécessaires pour rendre et hydrater la page elle-même) est
//! redirigée vers `/bootstrap`.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};

use crate::state::AppState;

/// Chemin de la page de bootstrap, exempté de la redirection.
const BOOTSTRAP_PATH: &str = "/bootstrap";
/// Préfixe des ressources statiques générées par `cargo-leptos`
/// (`site-pkg-dir`, voir `Cargo.toml`), toujours nécessaires au rendu.
const ASSETS_PREFIX: &str = "/pkg/";

fn is_allowed_during_bootstrap(path: &str) -> bool {
    path == BOOTSTRAP_PATH || path.starts_with(ASSETS_PREFIX)
}

/// Middleware appliqué à l'ensemble du routeur (voir `crate::run`) : redirige
/// vers `/bootstrap` tant que l'état bootstrap est actif.
pub async fn bootstrap_guard(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if state.bootstrap_required.load(Ordering::SeqCst)
        && !is_allowed_during_bootstrap(request.uri().path())
    {
        return Redirect::to(BOOTSTRAP_PATH).into_response();
    }
    next.run(request).await
}
