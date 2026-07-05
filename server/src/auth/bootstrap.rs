//! Route de bootstrap : création du compte super administrateur unique
//! (voir `Claude.md` § « Ajoute un état bootstrap... » et
//! `crate::guard::bootstrap_guard`).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::Form;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use axum_extra::extract::PrivateCookieJar;
use serde::Deserialize;

use storage::StorageError;

use crate::state::{AppState, SessionKey};

use super::session;

/// Longueur minimale imposée au mot de passe du compte de bootstrap.
const MIN_PASSWORD_LEN: usize = 12;

#[derive(Debug, Deserialize)]
pub struct BootstrapForm {
    pub email: String,
    pub display_name: String,
    pub password: String,
    pub password_confirmation: String,
}

/// Crée le compte super administrateur unique, ouvre une session et redirige
/// vers `/`. Redirige vers `/login` si l'état bootstrap est déjà terminé
/// (compte créé entretemps par une autre requête).
pub async fn create(
    State(state): State<Arc<AppState>>,
    jar: PrivateCookieJar<SessionKey>,
    Form(form): Form<BootstrapForm>,
) -> impl IntoResponse {
    if !state.bootstrap_required.load(Ordering::SeqCst) {
        return Redirect::to("/login").into_response();
    }

    let email = form.email.trim().to_string();
    let display_name = form.display_name.trim().to_string();
    if email.is_empty() || display_name.is_empty() {
        return Redirect::to("/bootstrap?error=champs").into_response();
    }
    if form.password.len() < MIN_PASSWORD_LEN || form.password != form.password_confirmation {
        return Redirect::to("/bootstrap?error=mot_de_passe").into_response();
    }

    match storage::bootstrap::create_super_administrator(
        &state.store,
        email,
        display_name,
        &form.password,
    )
    .await
    {
        Ok(user) => {
            // Termine l'état bootstrap dès la création du compte : les
            // requêtes suivantes ne sont plus redirigées vers `/bootstrap`
            // (voir `crate::guard::bootstrap_guard`).
            state.bootstrap_required.store(false, Ordering::SeqCst);
            match session::start_session(&state.store, &user.id).await {
                Ok(cookie) => (jar.add(cookie), Redirect::to("/")).into_response(),
                Err(err) => {
                    tracing::error!("{err}");
                    Redirect::to("/login").into_response()
                }
            }
        }
        Err(StorageError::AlreadyBootstrapped) => Redirect::to("/login").into_response(),
        Err(err) => {
            tracing::error!("{err}");
            Redirect::to("/bootstrap?error=indisponible").into_response()
        }
    }
}
