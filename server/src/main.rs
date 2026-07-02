#![recursion_limit = "256"]

mod ports;
mod protocol;
mod state;
mod ws;

use std::sync::Arc;
use agent::{LanguageModel, OpenAiCompatibleModel, OpenAiCompatibleModelConfig};

use state::{AppState, Rooms};

/// Construit le modèle de langage utilisé par la boucle agentique à partir
/// des variables d'environnement `AGENT_BASE_URL`/`AGENT_API_KEY`/`AGENT_MODEL`.
/// Renvoie `None` si l'une d'elles est absente : les requêtes `run_agent`
/// échoueront alors proprement plutôt que de faire planter le serveur au
/// démarrage.
fn build_language_model() -> Option<Arc<dyn LanguageModel>> {
    let base_url = std::env::var("AGENT_BASE_URL").ok()?;
    let api_key = std::env::var("AGENT_API_KEY").ok()?;
    let model = std::env::var("AGENT_MODEL").ok()?;
    Some(Arc::new(OpenAiCompatibleModel::new(OpenAiCompatibleModelConfig { base_url, api_key, model })))
}

#[tokio::main]
async fn main() {
    use axum::routing::get;
    use axum::Router;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use app::app::*;

    dotenv::dotenv().ok();
    
    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App
    let routes = generate_route_list(App);

    let model = build_language_model();
    if model.is_none() {
        log!(
            "AGENT_BASE_URL/AGENT_API_KEY/AGENT_MODEL non configurées : \
             la boucle agentique sera indisponible sur /ws/{{room_id}}"
        );
    }
    let ws_state = Arc::new(AppState { rooms: Rooms::default(), model });

    let leptos_router = Router::new()
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    let ws_router = Router::new().route("/ws/{room_id}", get(ws::ws_handler)).with_state(ws_state);

    let app = leptos_router.merge(ws_router);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
