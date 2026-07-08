use legal_act::{ConnectedUser, LegalActEditor};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    StaticSegment,
    components::{Route, Router, Routes},
    hooks::use_params_map,
    path,
};

use crate::pages::account::PageAccount;
use crate::pages::admin::{
    PageAdminAgentProfiles, PageAdminAgentTools, PageAdminAiModels, PageAdminAudit,
    PageAdminAuthorities, PageAdminDashboard, PageAdminDomains, PageAdminGroups,
    PageAdminIntegrations, PageAdminIntentions, PageAdminOidc, PageAdminUsers,
};
use crate::pages::bootstrap::PageBootstrap;
use crate::pages::dashboard::PageDashboard;
use crate::pages::dev::PageDevAgentPanel;
use crate::pages::editor_new::PageEditorNew;
use crate::pages::login::PageLogin;
use crate::pages::project_intentions::ProjectIntentionsPanel;
use crate::ws;

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
                    <Route path=StaticSegment("") view=PageDashboard/>
                    <Route path=path!("/login") view=PageLogin/>
                    <Route path=path!("/bootstrap") view=PageBootstrap/>
                    <Route path=path!("/account") view=PageAccount/>
                    <Route path=path!("/editor/new") view=PageEditorNew/>
                    <Route path=path!("/editor/:id") view=PageEditorProjet/>
                    <Route path=path!("/dev/agent") view=PageDevAgentPanel/>
                    <Route path=path!("/admin") view=PageAdminDashboard/>
                    <Route path=path!("/admin/users") view=PageAdminUsers/>
                    <Route path=path!("/admin/groups") view=PageAdminGroups/>
                    <Route path=path!("/admin/authorities") view=PageAdminAuthorities/>
                    <Route path=path!("/admin/domains") view=PageAdminDomains/>
                    <Route path=path!("/admin/intentions") view=PageAdminIntentions/>
                    <Route path=path!("/admin/agent-tools") view=PageAdminAgentTools/>
                    <Route path=path!("/admin/agent-profiles") view=PageAdminAgentProfiles/>
                    <Route path=path!("/admin/ai-models") view=PageAdminAiModels/>
                    <Route path=path!("/admin/oidc") view=PageAdminOidc/>
                    <Route path=path!("/admin/integrations") view=PageAdminIntegrations/>
                    <Route path=path!("/admin/audit") view=PageAdminAudit/>
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

/// Page `/editor/:id` : édition collaborative du projet d'acte légal `id`
/// (créé via `/editor/new`, voir [`crate::pages::editor_new::PageEditorNew`]).
///
/// L'identifiant de la route sert directement d'identifiant de salle de
/// collaboration WebRTC/WebSocket.
#[component]
fn PageEditorProjet() -> impl IntoView {
    let params = use_params_map();
    let room_id = params.get_untracked().get("id").unwrap_or_default();
    let legal_act_id = room_id.clone();
    let room = ws::connect_room(room_id);
    let identity = Resource::new(|| (), |_| editor_header_identity());

    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Connexion à la salle de collaboration…"</p> }>
            {move || {
                // `StoredValue` (Copy) plutôt qu'un `String` capturé
                // directement : `legal_act_id` est utilisé depuis la
                // fermeture `Fn` imbriquée des enfants de `<LegalActEditor>`
                // (onglet « Paramètres »), elle-même imbriquée dans celle de
                // `<Show>`.
                let legal_act_id = StoredValue::new(legal_act_id.clone());
                Suspend::new(async move {
                let identity = identity.await.ok();
                let user_initial = identity.as_ref().map(|identity| identity.initial.clone());
                let is_admin = identity.as_ref().is_some_and(|identity| identity.is_admin);
                let current_user_id = identity.as_ref().map(|identity| identity.user_id.clone());
                // Exclut l'utilisateur courant de la liste (sa propre bulle
                // d'avatar est déjà affichée séparément, voir `user_initial`).
                let connected_users_current_user_id = current_user_id.clone();
                let connected_users = Signal::derive(move || {
                    room.connected_users
                        .get()
                        .into_iter()
                        .filter(|user| Some(&user.user_id) != connected_users_current_user_id.as_ref())
                        .map(|user| ConnectedUser {
                            user_id: user.user_id,
                            initial: user.initial,
                            color: user.color,
                        })
                        .collect::<Vec<_>>()
                });

                view! {
                    <Show
                        when=move || room.ready.get()
                        fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Connexion à la salle de collaboration…"</p> }
                    >
                        <LegalActEditor
                            autorite="Préfet\nDe Normandie"
                            body=room.body
                            reviews=room.reviews
                            current_user=current_user_id.clone()
                            // TODO(permissions) : tant que le modèle de
                            // permissions par projet n'est pas branché
                            // jusqu'ici, tout utilisateur pouvant atteindre
                            // cette page (déjà authentifié, voir
                            // `editor_header_identity`) est considéré comme
                            // rédacteur, au même niveau de confiance que le
                            // reste de l'éditeur (aucun mode lecture seule
                            // n'y est encore appliqué non plus).
                            can_edit=true
                            agent_messages=room.agent_messages
                            agent_pending=room.agent_pending
                            on_agent_send=Callback::new(move |task| room.run_agent(task))
                            agent_interaction=room.interaction
                            on_agent_respond=Callback::new(move |resp| room.respond(resp))
                            agent_auto_accept=room.auto_accept
                            on_agent_toggle_auto_accept=Callback::new(move |enabled| room.set_auto_accept(enabled))
                            on_agent_clear_history=Callback::new(move |()| room.clear_history())
                            on_agent_target=Callback::new(move |node_id| room.set_selection(node_id))
                            agent_document_request=room.document_request
                            on_agent_document_response=Callback::new(move |upload| room.respond_document(upload))
                            agent_sessions=room.agent_sessions
                            on_list_agent_sessions=Callback::new(move |()| room.list_agent_sessions())
                            on_open_agent_session=Callback::new(move |session_id| room.open_agent_session(session_id))
                            agent_session_history=room.agent_session_history
                            on_close_agent_session_history=Callback::new(move |()| room.close_agent_session_history())
                            agent_supervisor_context=room.supervisor_context
                            on_view_agent_supervisor_context=Callback::new(move |()| room.view_supervisor_context())
                            on_close_agent_supervisor_context=Callback::new(move |()| room.close_supervisor_context())
                            user_initial=user_initial.clone()
                            is_admin=is_admin
                            connected_users=connected_users
                        >
                            <ProjectIntentionsPanel legal_act_id=legal_act_id.get_value()/>
                        </LegalActEditor>
                    </Show>
                }
                })
            }}
        </Suspense>
    }
}
