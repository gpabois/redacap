// La vue Leptos complète (App -> LegalActEditor -> ...) génère des types de
// composants profondément imbriqués ; la limite par défaut du vérificateur de
// types est dépassée (voir aussi `legal_act/src/lib.rs`, `server/src/main.rs`).
#![recursion_limit = "256"]

pub mod app;
pub mod component;
pub mod pages;
mod protocol;
pub mod utils;
pub mod ws;
