use std::sync::Arc;

use axum::{Router, routing::get};

use crate::state::AppState;

pub(crate) mod ports;
pub(crate) mod presence;
pub(crate) mod protocol;
pub(crate) mod state;
pub(crate) mod ws;

pub fn build(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/{id}/ws", get(ws::ws_handler))
        .with_state(state)
}
