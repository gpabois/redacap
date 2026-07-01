//! Alerte DSFR (`fr-alert`) : message contextuel de sévérité variable,
//! avec fermeture optionnelle.

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::components::common::Severity;

/// Message contextuel mettant en avant une information, un succès, un
/// avertissement ou une erreur.
#[component]
pub fn Alert(
    severity: Severity,
    #[prop(optional)] title: Option<&'static str>,
    #[prop(optional)] small: bool,
    #[prop(optional, into)] on_close: Option<Callback<MouseEvent>>,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    let padding = if small { "p-3" } else { "p-4" };
    view! {
        <div
            role="alert"
            class=format!(
                "{} {} {class} {padding} relative border-l-4 rounded-sm",
                severity.bg_class(),
                severity.border_class(),
            )
        >
            <p class=format!("{} font-bold mb-1", severity.text_class())>
                {title.unwrap_or_else(|| severity.default_label())}
            </p>
            <div class="text-sm text-gray-800">
                {children()}
            </div>
            {on_close.map(|cb| view! {
                <button
                    type="button"
                    class="absolute top-2 right-2 cursor-pointer text-gray-500 hover:text-gray-800"
                    aria-label="Masquer le message"
                    on:click=move |ev| cb.run(ev)
                >
                    "×"
                </button>
            })}
        </div>
    }
}
