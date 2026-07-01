//! Badge DSFR (`fr-badge`) : étiquette compacte de statut.

use leptos::prelude::*;

use crate::components::common::Severity;

/// Étiquette compacte indiquant un statut sémantique.
#[component]
pub fn Badge(
    severity: Severity,
    #[prop(optional)] small: bool,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    let size_class = if small {
        "text-xs px-1.5 py-0.5"
    } else {
        "text-sm px-2 py-1"
    };
    view! {
        <p class=format!(
            "{} {} {class} {size_class} inline-flex items-center font-bold rounded-sm w-fit",
            severity.bg_class(),
            severity.text_class(),
        )>
            {children()}
        </p>
    }
}
