//! Tag DSFR (`fr-tag`) : filtre ou mot-clé cliquable, éventuellement
//! sélectionnable ou amovible.

use leptos::ev::MouseEvent;
use leptos::prelude::*;

/// Mot-clé ou filtre. Si `selected` est fourni, le tag devient
/// sélectionnable (case d'usage : filtres de recherche).
#[component]
pub fn Tag(
    #[prop(optional)] selected: bool,
    #[prop(optional, into)] on_dismiss: Option<Callback<MouseEvent>>,
    #[prop(optional)] class: &'static str,
    on_click: impl Fn(MouseEvent) + 'static,
    children: Children,
) -> impl IntoView {
    let state_class = if selected {
        "bg-blue-france text-white"
    } else {
        "bg-gray-100 text-blue-france hover:bg-blue-france-975"
    };
    view! {
        <span class=format!("{class} {state_class} inline-flex items-center gap-1.5 text-sm rounded-full px-3 py-1 cursor-pointer transition-colors")
            on:click=on_click
        >
            {children()}
            {on_dismiss.map(|cb| view! {
                <button
                    type="button"
                    class="cursor-pointer leading-none"
                    aria-label="Retirer"
                    on:click=move |ev: MouseEvent| {
                        ev.stop_propagation();
                        cb.run(ev);
                    }
                >
                    "×"
                </button>
            })}
        </span>
    }
}

/// Regroupement de tags (`fr-tags-group`), par exemple une liste de filtres
/// actifs.
#[component]
pub fn TagGroup(#[prop(optional)] class: &'static str, children: Children) -> impl IntoView {
    view! {
        <ul class=format!("{class} flex flex-wrap gap-2 list-none")>
            {children()}
        </ul>
    }
}
