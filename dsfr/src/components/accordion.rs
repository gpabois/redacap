//! Accordéon DSFR (`fr-accordion`) : section repliable.

use leptos::prelude::*;

/// Section de contenu repliable, ouverte/fermée par clic sur son titre.
#[component]
pub fn Accordion(
    title: &'static str,
    #[prop(optional)] default_open: bool,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    let (open, set_open) = signal(default_open);
    view! {
        <div class=format!("{class} border-b border-gray-300 dark:border-gray-700")>
            <h3>
                <button
                    type="button"
                    aria-expanded=move || open.get().to_string()
                    class="w-full flex items-center justify-between gap-2 py-3 text-left font-bold text-blue-france dark:text-blue-france-925 cursor-pointer"
                    on:click=move |_| set_open.update(|open| *open = !*open)
                >
                    <span>{title}</span>
                    <span class=move || format!(
                        "transition-transform {}",
                        if open.get() { "rotate-180" } else { "" },
                    )>"⌄"</span>
                </button>
            </h3>
            <div class="pb-4" class:hidden=move || !open.get()>
                {children()}
            </div>
        </div>
    }
}

/// Regroupement vertical d'[`Accordion`].
#[component]
pub fn AccordionGroup(#[prop(optional)] class: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class=format!("{class} flex flex-col")>
            {children()}
        </div>
    }
}
