//! Pagination DSFR (`fr-pagination`).

use leptos::prelude::*;

fn page_button_class(active: bool) -> &'static str {
    if active {
        "bg-blue-france text-white"
    } else {
        "text-blue-france hover:bg-blue-france-975"
    }
}

/// Pagination pilotée par un signal de page courante (indexée à partir de 0).
#[component]
pub fn Pagination(
    current: RwSignal<usize>,
    total_pages: usize,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let total_pages = total_pages.max(1);
    view! {
        <nav role="navigation" aria-label="Pagination" class=format!("{class} flex items-center gap-1 text-sm")>
            <button
                type="button"
                class="px-3 py-2 rounded-sm cursor-pointer text-blue-france hover:bg-blue-france-975 disabled:opacity-40 disabled:cursor-not-allowed"
                disabled=move || current.get() == 0
                on:click=move |_| current.update(|p| *p = p.saturating_sub(1))
            >
                "‹ Précédent"
            </button>
            {(0..total_pages).map(|page| {
                view! {
                    <button
                        type="button"
                        aria-current=move || (current.get() == page).then_some("page")
                        class=move || format!("size-9 rounded-sm cursor-pointer {}", page_button_class(current.get() == page))
                        on:click=move |_| current.set(page)
                    >
                        {page + 1}
                    </button>
                }
            }).collect::<Vec<_>>()}
            <button
                type="button"
                class="px-3 py-2 rounded-sm cursor-pointer text-blue-france hover:bg-blue-france-975 disabled:opacity-40 disabled:cursor-not-allowed"
                disabled=move || current.get() + 1 >= total_pages
                on:click=move |_| current.update(|p| *p = (*p + 1).min(total_pages - 1))
            >
                "Suivant ›"
            </button>
        </nav>
    }
}
