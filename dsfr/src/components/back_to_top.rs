//! Bouton de retour en haut de page DSFR (`fr-link--top` / back-to-top).

use leptos::prelude::*;

/// Lien de retour vers une ancre de haut de page (typiquement `"#top"`).
#[component]
pub fn BackToTop(
    #[prop(optional, default = "top")] anchor: &'static str,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! {
        <a
            href=format!("#{anchor}")
            class=format!("{class} inline-flex items-center gap-2 text-sm font-bold text-blue-france hover:underline")
        >
            <span aria-hidden="true">"↑"</span>
            "Haut de page"
        </a>
    }
}
