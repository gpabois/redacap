//! Cookie de session opaque (voir contrainte racine « Authentification ») :
//! le cookie ne porte que l'identifiant de session, chiffré et authentifié
//! par `axum_extra::extract::cookie::PrivateCookieJar` (clé dérivée de
//! `SESSION_SECRET`) — il est donc illisible et infalsifiable côté client.

use std::str::FromStr;
use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::Utc;
use time::Duration as CookieDuration;

use shared::id::ID;
use shared::model::CreateSession;
use storage::Pool;

use crate::state::{AppState, SessionKey};

use super::AuthError;

/// Nom du cookie portant l'identifiant de session.
pub const COOKIE_NAME: &str = "session";
/// Durée de vie de la session, alignée sur la contrainte racine (« Cookie de
/// Session opaque d'une durée de vie de 24h »).
pub const SESSION_TTL_HOURS: i64 = 24;

/// Crée une session en base pour `user_id` et renvoie le cookie de session à
/// poser sur la réponse (`jar.add(cookie)`).
///
/// Ne prend pas le `PrivateCookieJar` en paramètre : la valeur serait déplacée
/// dès l'évaluation de l'expression `.await`, avant même le filtrage du
/// `Result`, ce qui empêcherait de la réutiliser dans la branche `Err` des
/// appelants (ex. `oidc::callback`, qui doit effacer le cookie de flux OIDC
/// même en cas d'échec de la création de session).
pub async fn start_session(pool: &Pool, user_id: &ID) -> Result<Cookie<'static>, AuthError> {
    let expires_at = Utc::now() + chrono::Duration::hours(SESSION_TTL_HOURS);
    let session = storage::session::create_session(
        pool,
        CreateSession {
            user_id: *user_id,
            expires_at,
        },
    )
    .await?;

    Ok(session_cookie(
        session.id.to_string(),
        SESSION_TTL_HOURS * 3600,
    ))
}

/// Construit le cookie de session HttpOnly + SameSite=Strict. La portée
/// `Strict` (plutôt que `Lax`) est possible ici car aucune navigation entrante
/// depuis un site tiers ne doit jamais présenter ce cookie : contrairement au
/// cookie transitoire du flux OIDC (`oidc::FLOW_COOKIE_NAME`), il n'y a pas de
/// redirection externe à faire aboutir.
fn session_cookie(value: String, max_age_secs: i64) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, value))
        .http_only(true)
        .same_site(SameSite::Strict)
        // Non sécurisé en dev (`cargo run`, HTTP local) pour ne pas casser le
        // flux de connexion sans certificat TLS local.
        .secure(!cfg!(debug_assertions))
        .path("/")
        .max_age(CookieDuration::seconds(max_age_secs))
        .build()
}

/// Déconnecte l'utilisateur courant : invalide la session en base et efface le cookie.
pub async fn logout(
    State(state): State<Arc<AppState>>,
    jar: PrivateCookieJar<SessionKey>,
) -> impl IntoResponse {
    if let Some(cookie) = jar.get(COOKIE_NAME) {
        if let Ok(session_id) = ID::from_str(cookie.value()) {
            let _ = storage::session::delete_session(&state.store, &session_id).await;
        }
    }
    let jar = jar.remove(Cookie::from(COOKIE_NAME));
    (jar, Redirect::to("/login")).into_response()
}
