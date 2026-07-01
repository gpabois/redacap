//! Bandeau d'information DSFR (`fr-notice`) : message pleine largeur en
//! tête de page, fermable.

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::components::common::Severity;

/// Bandeau d'information pleine largeur, généralement affiché en tête de
/// page (maintenance, information générale).
#[component]
pub fn Notice(
    #[prop(optional)] severity: Option<Severity>,
    #[prop(optional, into)] on_close: Option<Callback<MouseEvent>>,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    let severity = severity.unwrap_or(Severity::Info);
    view! {
        <div class=format!("{} {} {class} w-full px-4 py-2 flex items-center gap-4", severity.bg_class(), severity.text_class())>
            <p class="flex-1 text-sm font-bold">
                {children()}
            </p>
            {on_close.map(|cb| view! {
                <button
                    type="button"
                    class="cursor-pointer shrink-0"
                    aria-label="Masquer le message"
                    on:click=move |ev| cb.run(ev)
                >
                    "×"
                </button>
            })}
        </div>
    }
}
