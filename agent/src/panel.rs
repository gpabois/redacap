//! Panneau de discussion avec l'agent IA, sous forme de composant Leptos
//! autonome : il ignore tout de la boucle agentique ou du modèle de
//! langage utilisés, et ne fait qu'afficher un historique de messages et
//! relayer la saisie de l'utilisateur via `on_send`. La page hôte reste
//! responsable de l'appel réel à l'agent (typiquement via une fonction
//! serveur Leptos) et de la mise à jour de `messages`/`pending` en retour.

use leptos::*;
use leptos::prelude::*;
use web_sys::KeyboardEvent;

/// Rôle de l'émetteur d'un message affiché dans le panneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelRole {
    User,
    Assistant,
}

/// Message affiché dans l'historique du panneau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelMessage {
    pub role: PanelRole,
    pub content: String,
}

impl PanelMessage {
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: PanelRole::User, content: content.into() }
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: PanelRole::Assistant, content: content.into() }
    }
}

/// Panneau de discussion avec l'agent IA : historique des échanges, zone de
/// saisie et indicateur d'attente pendant l'exécution de la boucle
/// agentique côté serveur.
#[component]
pub fn AgentPanel(
    /// Historique des messages échangés, tenu par la page hôte.
    #[prop(into)] messages: Signal<Vec<PanelMessage>>,
    /// `true` tant que l'agent n'a pas renvoyé sa réponse finale.
    #[prop(into)] pending: Signal<bool>,
    /// Invoqué avec le texte saisi lorsque l'utilisateur envoie un message.
    on_send: impl Fn(String) + Clone + Send + 'static,
) -> impl IntoView {
    let (draft, set_draft) = signal(String::new());

    let send = move || {
        let text = draft.get().trim().to_string();
        if text.is_empty() || pending.get() {
            return;
        }
        on_send(text);
        set_draft.set(String::new());
    };
    let send_on_click = send.clone();

    view! {
        <div class="flex flex-col h-full border border-teal-600 rounded-sm">
            <div class="px-2 py-1 text-xs font-semibold border-b border-teal-600 bg-teal-50">
                "Agent IA"
            </div>
            <div class="flex-1 overflow-y-auto p-2 flex flex-col gap-2">
                <For
                    each=move || messages.get().into_iter().enumerate()
                    key=|(index, _)| *index
                    children=move |(_, message)| {
                        let alignment = match message.role {
                            PanelRole::User => "self-end bg-teal-600 text-white",
                            PanelRole::Assistant => "self-start bg-stone-100 text-stone-900",
                        };
                        view! {
                            <div class=format!("max-w-[80%] rounded-sm px-2 py-1 text-sm {alignment}")>
                                {message.content.clone()}
                            </div>
                        }
                    }
                />
                {move || pending.get().then(|| view! {
                    <div class="self-start max-w-[80%] rounded-sm px-2 py-1 text-sm bg-stone-100 text-stone-400 italic">
                        "L'agent réfléchit…"
                    </div>
                })}
            </div>
            <div class="flex border-t border-teal-600">
                <input
                    type="text"
                    class="flex-1 p-1 text-sm outline-none"
                    placeholder="Demander à l'agent…"
                    prop:value=draft
                    prop:disabled=move || pending.get()
                    on:input=move |ev| set_draft.set(event_target_value(&ev))
                    on:keydown=move |ev: KeyboardEvent| {
                        if ev.key() == "Enter" {
                            send();
                        }
                    }
                />
                <button
                    type="button"
                    class="px-2 text-xs text-teal-600 disabled:text-stone-300"
                    disabled=move || pending.get() || draft.get().trim().is_empty()
                    on:click=move |_| send_on_click()
                >
                    "Envoyer"
                </button>
            </div>
        </div>
    }
}
