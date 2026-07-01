//! Tuile DSFR (`fr-tile`) : carte cliquable compacte, souvent utilisée
//! comme point d'entrée vers une fonctionnalité.

use leptos::prelude::*;

/// Carte cliquable compacte, point d'entrée vers une page ou une action.
#[component]
pub fn Tile(
    title: &'static str,
    href: String,
    #[prop(optional)] description: Option<&'static str>,
    #[prop(optional)] pictogram: Option<&'static str>,
    #[prop(optional)] horizontal: bool,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let layout = if horizontal {
        "flex-row items-center"
    } else {
        "flex-col"
    };
    view! {
        <a
            href=href
            class=format!("{class} {layout} flex gap-4 p-6 border border-gray-300 hover:bg-blue-france-975 transition-colors")
        >
            {pictogram.map(|pictogram| view! { <span class="text-3xl shrink-0">{pictogram}</span> })}
            <span class="flex flex-col gap-1">
                <span class="font-bold text-blue-france">{title}</span>
                {description.map(|description| view! { <span class="text-sm text-gray-700">{description}</span> })}
            </span>
        </a>
    }
}
