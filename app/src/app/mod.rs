use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
    path,
};
use legal_act::{Body, DirectBody, LegalActEditor};

use crate::pages::dev::PageDevAgentPanel;

/// Shell HTML complet pour le rendu SSR (injecté dans leptos_axum).
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="fr">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
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
            <main class="min-h-screen bg-white">
                <Routes fallback=|| view! { <p class="p-8">"Page introuvable."</p> }>
                    <Route path=StaticSegment("") view=PageEditeurActe/>
                    <Route path=path!("/dev/agent") view=PageDevAgentPanel/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn PageEditeurActe() -> impl IntoView {
    let body = Body::from(DirectBody::new());
    view! {
        <LegalActEditor body=body/>
    }
}
