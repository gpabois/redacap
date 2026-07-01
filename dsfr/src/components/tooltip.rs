//! Info-bulle DSFR (`fr-tooltip`) : message contextuel affiché au survol
//! ou au focus du déclencheur.

use leptos::prelude::*;

/// Message contextuel affiché au survol ou au focus de son contenu.
#[component]
pub fn Tooltip(
    text: &'static str,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    let (visible, set_visible) = signal(false);
    view! {
        <span
            class=format!("{class} relative inline-flex")
            on:mouseenter=move |_| set_visible.set(true)
            on:mouseleave=move |_| set_visible.set(false)
            on:focusin=move |_| set_visible.set(true)
            on:focusout=move |_| set_visible.set(false)
        >
            {children()}
            <span
                role="tooltip"
                class="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 w-max max-w-xs px-3 py-2 rounded-sm bg-gray-900 text-white text-xs z-10"
                class:hidden=move || !visible.get()
            >
                {text}
            </span>
        </span>
    }
}
