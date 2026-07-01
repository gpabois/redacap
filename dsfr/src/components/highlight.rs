//! Mise en exergue DSFR (`fr-highlight`) : bloc de texte court signalé par
//! une bordure gauche, sans encart.

use leptos::prelude::*;

/// Bloc de texte mis en exergue par une bordure gauche.
#[component]
pub fn Highlight(#[prop(optional)] class: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class=format!("{class} border-l-4 border-blue-france pl-4 py-1 text-lg")>
            {children()}
        </div>
    }
}
