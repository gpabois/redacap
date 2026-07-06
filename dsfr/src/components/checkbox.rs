//! Case à cocher DSFR (`fr-checkbox`).

use leptos::ev::Event;
use leptos::prelude::*;

/// Case à cocher avec libellé.
#[component]
pub fn Checkbox(
    label: &'static str,
    #[prop(into)] checked: Signal<bool>,
    on_change: impl Fn(bool) + 'static,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! {
        <label class=format!("{class} flex items-center gap-2 text-sm cursor-pointer disabled:opacity-50")>
            <input
                type="checkbox"
                class="size-5 accent-blue-france cursor-pointer"
                disabled=disabled
                prop:checked=move || checked.get()
                on:change=move |ev: Event| on_change(event_target_checked(&ev))
            />
            {label}
        </label>
    }
}

/// Regroupement de cases à cocher liées, avec légende.
#[component]
pub fn CheckboxGroup(
    legend: &'static str,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <fieldset class=format!("{class} flex flex-col gap-2")>
            <legend class="text-base font-bold text-gray-900 dark:text-gray-100 mb-1">{legend}</legend>
            {children()}
        </fieldset>
    }
}
