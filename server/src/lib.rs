#![recursion_limit = "256"]

mod auth;
mod editor;
mod guard;
pub mod state;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use axum::Router;
use axum_extra::extract::cookie::Key;
use leptos::logging::log;
use leptos::prelude::*;
use leptos_axum::{LeptosRoutes, generate_route_list};

use app::app::*;
use log::error;
use state::AppState;

use crate::editor::state::EditorRooms;

/// Décode la clé de chiffrement/déchiffrement des secrets applicatifs
/// (`client_secret` OIDC, clé API des modèles IA, clés GéoRisques/Légifrance)
/// depuis la variable d'environnement `SECRET_ENCRYPTION_KEY` (au moins 32
/// octets encodés en base64 standard — AES-256 exige une clé de 32 octets ;
/// si la valeur décodée est plus longue, seuls les 32 premiers octets sont
/// conservés). Renvoie `None` si elle est absente ou invalide : les
/// fonctionnalités concernées sont alors indisponibles plutôt que de faire
/// planter le serveur au démarrage.
fn build_secret_encryption_key() -> Option<Vec<u8>> {
    use base64::Engine;
    let encoded = std::env::var("SECRET_ENCRYPTION_KEY")
    .inspect_err(|err| {
        error!("Erreur lors de la recherche du SECRET_ENCRYPTION_KEY: {err}")
    }).ok()?;

    let mut bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .inspect_err(|err| {
            error!("Erreur lors du décodage du SECRET_ENCRYPTION_KEY: {err}")
        })
        .ok()?;

    if bytes.len() < 32 {
        error!(
            "SECRET_ENCRYPTION_KEY doit faire au moins 32 octets une fois décodée (obtenu : {} octets)",
            bytes.len()
        );
        return None;
    }

    bytes.truncate(32);
    Some(bytes)
}

/// Démarre le serveur applicatif : rendu SSR, API privée (ServerFunctions,
/// Websockets) et API publique.
pub async fn run() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let conf = get_configuration(None)?;
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App
    let routes = generate_route_list(App);

    let database_url = std::env::var("DATABASE_URL")?;
    let store = storage::connect(&database_url).await?;

    // État bootstrap (voir `Claude.md` § « Ajoute un état bootstrap... ») :
    // évalué une fois au démarrage, puis maintenu en mémoire (voir
    // `state::AppState::bootstrap_required`) plutôt que reconsulté à chaque
    // requête.
    let bootstrap_required = Arc::new(AtomicBool::new(
        storage::bootstrap::is_required(&store).await?,
    ));

    let session_secret = std::env::var("SESSION_SECRET")
        .map_err(|_| anyhow::anyhow!("SESSION_SECRET doit être définie (≥32 octets)"))?;
    let session_key = Key::derive_from(session_secret.as_bytes());

    let secret_encryption_key = build_secret_encryption_key();
    let public_base_url = std::env::var("PUBLIC_BASE_URL").ok();
    if secret_encryption_key.is_none() {
        log!(
            "SECRET_ENCRYPTION_KEY non configurée : l'authentification par fournisseur OpenID \
             Connect ainsi que les modèles IA / intégrations GéoRisques / Légifrance enregistrés \
             en base seront indisponibles (leurs secrets ne peuvent pas être déchiffrés)"
        );
    }
    if public_base_url.is_none() {
        log!(
            "PUBLIC_BASE_URL non configurée : l'authentification par fournisseur OpenID Connect sera indisponible"
        );
    }
    // `Policy::none()` : le client ne suit jamais les redirections HTTP,
    // afin d'éviter qu'un fournisseur OIDC malveillant ou compromis ne
    // détourne les requêtes de découverte/échange de jeton (protection SSRF).
    let oidc_http_client = openidconnect::reqwest::ClientBuilder::new()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()?;

    let app_state = Arc::new(AppState {
        rooms: EditorRooms::default(),
        store,
        session_key,
        secret_encryption_key,
        public_base_url,
        oidc_http_client,
        bootstrap_required,
    });

    let leptos_router = Router::new()
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let store = app_state.store.clone();
                let session_key = app_state.session_key.clone();
                let secret_encryption_key = app_state.secret_encryption_key.clone();
                let public_base_url = app_state.public_base_url.clone();
                move || {
                    provide_context(store.clone());
                    provide_context(session_key.clone());
                    provide_context(secret_encryption_key.clone());
                    provide_context(public_base_url.clone());
                }
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    let app = leptos_router
        .merge(auth::build(app_state.clone()))
        .nest("/editor", editor::build(app_state.clone()))
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            guard::bootstrap_guard,
        ));

    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
