use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::pages::dev::PageDevLegalActEditor;


/// Shell HTML complet pour le rendu SSR (injecté dans leptos_axum).
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="fr">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <link rel="icon" type="image/svg+xml" href="/favicon.svg"/>
                // Posé en premier et exécuté de façon synchrone, avant le
                // chargement du WASM : applique la classe `dark` sur `<html>`
                // d'après la préférence persistée pour éviter un flash du
                // mauvais thème (voir `dsfr::THEME_INIT_SCRIPT`).
                <script inner_html=dsfr::THEME_INIT_SCRIPT></script>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

/// Racine de l'application Leptos (SSR + hydratation).
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/redacap.css"/>
        <Title text="Redac'AP"/>
        <Router>
            <main class="min-h-screen bg-white dark:bg-gray-950 dark:text-gray-100">
                <Routes fallback=|| view! { <p class="p-8">"Page introuvable."</p> }>
                    <Route path=path!("/dev/editor") view=PageDevLegalActEditor/>
                </Routes>
            </main>
        </Router>
    }
}

/// Identité affichée dans la bulle d'avatar de l'en-tête de l'éditeur (voir
/// [`legal_act::LegalActEditor`]'s `user_initial`/`is_admin`).
#[server]
async fn editor_header_identity() -> Result<crate::auth::HeaderIdentity, ServerFnError> {
    let user_id = match crate::auth::current_user_id().await {
        Ok(user_id) => user_id,
        Err(error) => {
            leptos_axum::redirect("/login");
            return Err(error);
        }
    };

    let pool = expect_context::<storage::Pool>();
    crate::auth::header_identity(&pool, &user_id).await
}

