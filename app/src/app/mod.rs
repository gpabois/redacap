use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
    path,
};
use legal_act::LegalActEditor;

use crate::pages::dev::PageDevAgentPanel;
use crate::ws;

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

/// Identifiant du salon de collaboration rejoint par cette page.
///
/// Fixe pour l'instant : il n'existe pas encore de gestion de plusieurs
/// projets/actes côté application (routage, liste de projets...). Cette
/// page collabore donc toujours sur le même document ; faire évoluer ceci
/// vers un identifiant par projet (ex. route `/actes/:id`) est une
/// extension naturelle une fois ce routage introduit.
const ROOM_ID: &str = "demo";

#[component]
fn PageEditeurActe() -> impl IntoView {
    let room = ws::connect_room(ROOM_ID);

    view! {
        <Show
            when=move || room.ready.get()
            fallback=|| view! { <p class="p-8 text-gray-500">"Connexion à la salle de collaboration…"</p> }
        >
            <LegalActEditor
                autorite="Préfet\nDe Normandie"
                body=room.body
                agent_messages=room.agent_messages
                agent_pending=room.agent_pending
                on_agent_send=Callback::new(move |task| room.run_agent(task))
                agent_interaction=room.interaction
                on_agent_respond=Callback::new(move |resp| room.respond(resp))
                agent_auto_accept=room.auto_accept
                on_agent_toggle_auto_accept=Callback::new(move |enabled| room.set_auto_accept(enabled))
                on_agent_target=Callback::new(move |node_id| room.set_selection(node_id))
            />
        </Show>
    }
}
