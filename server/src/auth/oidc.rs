//! Connexion par fournisseur OpenID Connect (code d'autorisation + PKCE).
//!
//! Le flux est intégralement sans état côté serveur entre la redirection
//! (`start`) et le retour du fournisseur (`callback`) : le triplet
//! `(csrf, nonce, pkce_verifier)` transite dans un cookie chiffré transitoire
//! (`FLOW_COOKIE_NAME`, 5 minutes), plutôt que dans une table dédiée, en
//! réutilisant la même clé que le cookie de session.

use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet, EndpointSet, IssuerUrl,
    Nonce, OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse,
};
use serde::{Deserialize, Serialize};
use time::Duration as CookieDuration;

use shared::id::ID;
use shared::model::{CreateUser, OidcProvider};
use storage::Pool;

use crate::state::{AppState, SessionKey};

use super::{AuthError, crypto, session};

const FLOW_COOKIE_NAME: &str = "oidc_flow";
const FLOW_TTL_SECS: i64 = 300;

/// Client OIDC pleinement configuré par [`build_client`] : endpoint
/// d'autorisation (`from_provider_metadata`), de jeton et d'informations
/// utilisateur (`build_client`, exigés — le fournisseur doit les publier via
/// la découverte). Les endpoints device/introspection/révocation restent
/// non configurés (`EndpointNotSet`), aucun des flux utilisés ici n'en a besoin.
type OidcClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
    EndpointSet,
>;

/// État du flux OIDC en cours, porté par le cookie transitoire `FLOW_COOKIE_NAME`.
#[derive(Debug, Serialize, Deserialize)]
struct FlowState {
    csrf: String,
    nonce: String,
    pkce_verifier: String,
    provider_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Démarre le flux : redirige l'utilisateur vers l'écran d'authentification du
/// fournisseur `provider_id`.
pub async fn start(
    State(state): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    jar: PrivateCookieJar<SessionKey>,
) -> impl IntoResponse {
    match build_authorize_redirect(&state, &provider_id).await {
        Ok((redirect_url, flow)) => match flow_cookie(&flow) {
            Ok(cookie) => (jar.add(cookie), Redirect::to(&redirect_url)).into_response(),
            Err(_) => Redirect::to("/login?error=oidc").into_response(),
        },
        Err(_) => Redirect::to("/login?error=oidc").into_response(),
    }
}

/// Termine le flux : échange le code d'autorisation, vérifie le jeton d'identité,
/// résout (ou provisionne) l'utilisateur local puis ouvre sa session.
pub async fn callback(
    State(state): State<Arc<AppState>>,
    Path(provider_id): Path<String>,
    Query(params): Query<CallbackParams>,
    jar: PrivateCookieJar<SessionKey>,
) -> impl IntoResponse {
    let flow_cookie_value = jar.get(FLOW_COOKIE_NAME);
    let jar = jar.remove(Cookie::from(FLOW_COOKIE_NAME));

    match handle_callback(&state, &provider_id, &params, flow_cookie_value).await {
        Ok(user_id) => match session::start_session(&state.store, &user_id).await {
            Ok(cookie) => (jar.add(cookie), Redirect::to("/")).into_response(),
            Err(_) => (jar, Redirect::to("/login?error=indisponible")).into_response(),
        },
        Err(_) => (jar, Redirect::to("/login?error=oidc")).into_response(),
    }
}

async fn handle_callback(
    state: &AppState,
    provider_id_path: &str,
    params: &CallbackParams,
    flow_cookie_value: Option<Cookie<'static>>,
) -> Result<ID, AuthError> {
    if let Some(error) = &params.error {
        return Err(AuthError::Oidc(format!(
            "refusé par le fournisseur : {error}"
        )));
    }
    let code = params
        .code
        .clone()
        .ok_or_else(|| AuthError::Oidc("code d'autorisation manquant".to_string()))?;
    let returned_state = params
        .state
        .clone()
        .ok_or_else(|| AuthError::Oidc("paramètre state manquant".to_string()))?;

    let flow_cookie_value = flow_cookie_value
        .ok_or_else(|| AuthError::Oidc("session de connexion expirée".to_string()))?;
    let flow: FlowState = serde_json::from_str(flow_cookie_value.value())
        .map_err(|_| AuthError::Oidc("état de connexion invalide".to_string()))?;

    if flow.csrf != returned_state || flow.provider_id != provider_id_path {
        return Err(AuthError::Oidc("jeton anti-CSRF invalide".to_string()));
    }

    let provider_id = ID::from_str(provider_id_path).map_err(|_| AuthError::OidcUnavailable)?;
    let (client, _provider) = build_client(state, &provider_id).await?;

    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))
        .request_async(&state.oidc_http_client)
        .await
        .map_err(|err| AuthError::Oidc(err.to_string()))?;

    let id_token = token_response
        .id_token()
        .ok_or_else(|| AuthError::Oidc("jeton d'identité absent de la réponse".to_string()))?;
    let nonce = Nonce::new(flow.nonce);
    let claims = id_token
        .claims(&client.id_token_verifier(), &nonce)
        .map_err(|err| AuthError::Oidc(err.to_string()))?;

    let email = match claims.email() {
        Some(email) => email.as_str().to_string(),
        None => {
            let userinfo: openidconnect::core::CoreUserInfoClaims = client
                .user_info(token_response.access_token().to_owned(), None)
                .request_async(&state.oidc_http_client)
                .await
                .map_err(|err| AuthError::Oidc(err.to_string()))?;
            userinfo
                .email()
                .map(|email| email.as_str().to_string())
                .ok_or_else(|| {
                    AuthError::Oidc("email absent des informations du fournisseur".to_string())
                })?
        }
    };

    let display_name = claims
        .name()
        .and_then(|names| names.get(None))
        .map(|name| name.as_str().to_string())
        .unwrap_or_else(|| email.clone());

    resolve_or_create_user(&state.store, &email, &display_name).await
}

/// Résout l'utilisateur local associé à `email`, ou le provisionne à la volée
/// s'il s'agit de sa première connexion via ce fournisseur. Le compte créé ne
/// porte aucun droit (principe du moindre privilège, cf. `Claude.md`) : un
/// administrateur doit ensuite lui accorder les permissions nécessaires.
async fn resolve_or_create_user(
    pool: &Pool,
    email: &str,
    display_name: &str,
) -> Result<ID, AuthError> {
    match storage::user::get_user_by_email(pool, email).await {
        Ok(user) if user.suspended_at.is_none() => Ok(user.id),
        Ok(_suspended) => Err(AuthError::InvalidCredentials),
        Err(storage::StorageError::NotFound) => {
            let user = storage::user::create_user(
                pool,
                CreateUser {
                    email: email.to_string(),
                    display_name: display_name.to_string(),
                },
            )
            .await?;
            Ok(user.id)
        }
        Err(err) => Err(err.into()),
    }
}

async fn build_authorize_redirect(
    state: &AppState,
    provider_id_str: &str,
) -> Result<(String, FlowState), AuthError> {
    let provider_id = ID::from_str(provider_id_str).map_err(|_| AuthError::OidcUnavailable)?;
    let (client, provider) = build_client(state, &provider_id).await?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_request = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .set_pkce_challenge(pkce_challenge);

    for scope in authorize_scopes(&provider) {
        auth_request = auth_request.add_scope(Scope::new(scope));
    }

    let (auth_url, csrf_token, nonce) = auth_request.url();

    Ok((
        auth_url.to_string(),
        FlowState {
            csrf: csrf_token.secret().clone(),
            nonce: nonce.secret().clone(),
            pkce_verifier: pkce_verifier.secret().clone(),
            provider_id: provider_id_str.to_string(),
        },
    ))
}

/// Portées demandées au fournisseur : celles configurées pour `provider`
/// (`email`/`profile` par défaut si aucune n'est renseignée), en garantissant
/// systématiquement la présence de `openid`.
fn authorize_scopes(provider: &OidcProvider) -> Vec<String> {
    let mut scopes = provider.scopes.clone();
    if scopes.is_empty() {
        scopes = vec!["email".to_string(), "profile".to_string()];
    }
    if !scopes.iter().any(|scope| scope == "openid") {
        scopes.insert(0, "openid".to_string());
    }
    scopes
}

async fn build_client(
    state: &AppState,
    provider_id: &ID,
) -> Result<(OidcClient, OidcProvider), AuthError> {
    let encryption_key = state
        .secret_encryption_key
        .ok_or(AuthError::OidcUnavailable)?;
    let base_url = state
        .public_base_url
        .as_ref()
        .ok_or(AuthError::OidcUnavailable)?;

    let provider = storage::oidc_provider::get_oidc_provider(&state.store, provider_id).await?;
    if !provider.active {
        return Err(AuthError::OidcUnavailable);
    }
    let client_secret = crypto::decrypt(&encryption_key, &provider.client_secret_encrypted)?;

    let redirect_uri = format!("{base_url}/oidc/{provider_id}/callback");

    let metadata = CoreProviderMetadata::discover_async(
        IssuerUrl::new(provider.issuer_url.clone())
            .map_err(|err| AuthError::Oidc(err.to_string()))?,
        &state.oidc_http_client,
    )
    .await
    .map_err(|err| AuthError::Oidc(err.to_string()))?;

    // La découverte ne garantit statiquement (typestate `EndpointSet`) que
    // l'endpoint d'autorisation ; `token_endpoint`/`userinfo_endpoint` sont
    // `Option` dans les métadonnées bien qu'exigés en pratique par tous les
    // fournisseurs conformes utilisés ici (code d'autorisation + userinfo).
    let token_url = metadata.token_endpoint().cloned().ok_or_else(|| {
        AuthError::Oidc("le fournisseur ne publie pas d'endpoint de jeton".to_string())
    })?;
    let userinfo_url = metadata.userinfo_endpoint().cloned().ok_or_else(|| {
        AuthError::Oidc(
            "le fournisseur ne publie pas d'endpoint d'informations utilisateur".to_string(),
        )
    })?;

    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(provider.client_id.clone()),
        Some(ClientSecret::new(client_secret)),
    )
    .set_redirect_uri(
        RedirectUrl::new(redirect_uri).map_err(|err| AuthError::Oidc(err.to_string()))?,
    )
    .set_token_uri(token_url)
    .set_user_info_url(userinfo_url);

    Ok((client, provider))
}

fn flow_cookie(flow: &FlowState) -> Result<Cookie<'static>, AuthError> {
    let value = serde_json::to_string(flow).map_err(|err| AuthError::Oidc(err.to_string()))?;
    Ok(Cookie::build((FLOW_COOKIE_NAME, value))
        .http_only(true)
        // `Lax` (et non `Strict`, contrairement au cookie de session) : ce
        // cookie doit survivre à la navigation de premier niveau qui ramène
        // l'utilisateur depuis le fournisseur OIDC externe vers le callback.
        .same_site(SameSite::Lax)
        .secure(!cfg!(debug_assertions))
        .path("/oidc")
        .max_age(CookieDuration::seconds(FLOW_TTL_SECS))
        .build())
}
