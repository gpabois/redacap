//! Connexion par identifiants (email + mot de passe).

use std::sync::Arc;

use axum::Form;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use axum_extra::extract::PrivateCookieJar;
use serde::Deserialize;

use shared::id::ID;
use storage::Pool;

use crate::state::{AppState, SessionKey};

use super::AuthError;
use super::session;

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

/// Authentifie un utilisateur par email/mot de passe, crée une session et pose
/// le cookie de session correspondant. En cas d'échec, redirige vers `/login`
/// avec un paramètre `error` générique : ne jamais distinguer « email inconnu »
/// de « mot de passe invalide » côté réponse, afin de ne pas faciliter
/// l'énumération de comptes.
pub async fn login(
    State(state): State<Arc<AppState>>,
    jar: PrivateCookieJar<SessionKey>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    match authenticate(&state.store, &form).await {
        Ok(user_id) => match session::start_session(&state.store, &user_id).await {
            Ok(cookie) => (jar.add(cookie), Redirect::to("/")).into_response(),
            Err(err) => {
                tracing::error!("{err}");
                Redirect::to("/login?error=indisponible").into_response()
            }
        },
        Err(_) => Redirect::to("/login?error=identifiants").into_response(),
    }
}

async fn authenticate(pool: &Pool, form: &LoginForm) -> Result<ID, AuthError> {
    let user = storage::user::get_user_by_email(pool, &form.email)
        .await
        .map_err(|_| AuthError::InvalidCredentials)?;
    if user.suspended_at.is_some() {
        return Err(AuthError::InvalidCredentials);
    }
    let verified = storage::credential::verify_password(pool, &user.id, &form.password).await?;
    if verified {
        Ok(user.id)
    } else {
        Err(AuthError::InvalidCredentials)
    }
}
