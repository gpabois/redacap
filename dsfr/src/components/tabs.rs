//! Onglets DSFR (`fr-tabs`). [`Tabs`] affiche la liste des titres,
//! [`TabPanel`] est à utiliser par l'appelant pour chaque contenu associé.

use leptos::prelude::*;

/// Liste d'onglets cliquables piloté par un signal d'index sélectionné.
#[component]
pub fn Tabs(
    titles: Vec<&'static str>,
    selected: RwSignal<usize>,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! {
        <div role="tablist" class=format!("{class} flex border-b border-gray-300 overflow-x-auto")>
            {titles.into_iter().enumerate().map(|(i, title)| {
                view! {
                    <button
                        type="button"
                        role="tab"
                        aria-selected=move || (selected.get() == i).to_string()
                        class=move || format!(
                            "px-4 py-2 text-sm font-bold border-b-2 cursor-pointer whitespace-nowrap transition-colors {}",
                            if selected.get() == i {
                                "border-blue-france text-blue-france"
                            } else {
                                "border-transparent text-gray-600 hover:text-blue-france"
                            },
                        )
                        on:click=move |_| selected.set(i)
                    >
                        {title}
                    </button>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}

/// Panneau de contenu associé à l'onglet d'indice `index`.
#[component]
pub fn TabPanel(
    index: usize,
    selected: RwSignal<usize>,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <div
            role="tabpanel"
            class=format!("{class}")
            class:hidden=move || selected.get() != index
        >
            {children()}
        </div>
    }
}
