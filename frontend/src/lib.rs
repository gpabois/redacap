// La vue Leptos complète (App -> LegalActEditor -> ...) génère des types de
// composants profondément imbriqués ; la limite par défaut du vérificateur de
// types est dépassée (voir aussi `legal_act/src/lib.rs`, `server/src/main.rs`).
#![recursion_limit = "256"]

#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use app::app::*;
    // initializes logging using the `log` crate
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();

    leptos::mount::hydrate_body(App);
}
