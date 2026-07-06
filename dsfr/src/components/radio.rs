//! Bouton radio DSFR (`fr-radio`).

use leptos::prelude::*;

/// Bouton radio avec libellé. Le regroupement (même `name`) est à la
/// charge de l'appelant via [`RadioGroup`].
#[component]
pub fn Radio(
    label: &'static str,
    name: &'static str,
    #[prop(optional)] selected: bool,
    on_select: impl Fn() + 'static,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! {
        <label class=format!("{class} flex items-center gap-2 text-sm cursor-pointer disabled:opacity-50")>
            <input
                type="radio"
                name=name
                class="size-5 accent-blue-france cursor-pointer"
                disabled=disabled
                prop:checked=selected
                on:change=move |_| on_select()
            />
            {label}
        </label>
    }
}

/// Regroupement de boutons radio liés, avec légende.
#[component]
pub fn RadioGroup(
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
