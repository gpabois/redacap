//! Sélecteur DSFR (`fr-select`) : liste déroulante native stylisée.

use leptos::ev::Event;
use leptos::prelude::*;

/// Option d'un [`Select`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl SelectOption {
    /// Construit une option à partir d'une valeur et d'un libellé.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// Liste déroulante native, stylisée à la charte DSFR.
#[component]
pub fn Select(
    label: &'static str,
    options: Vec<SelectOption>,
    #[prop(into)] value: Signal<String>,
    on_change: impl Fn(String) + 'static,
    #[prop(optional)] hint: Option<&'static str>,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! {
        <div class=format!("{class} flex flex-col gap-1")>
            <label class="text-sm font-bold text-gray-900">{label}</label>
            {hint.map(|hint| view! { <span class="text-sm text-gray-600">{hint}</span> })}
            <select
                class="shadow-[inset_0_0_0_1px] shadow-gray-400 focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france bg-gray-100 px-3 py-2 outline-none disabled:opacity-50"
                disabled=disabled
                prop:value=move || value.get()
                on:change=move |ev: Event| on_change(event_target_value(&ev))
            >
                {options.into_iter().map(|opt| view! {
                    <option value=opt.value.clone()>{opt.label}</option>
                }).collect::<Vec<_>>()}
            </select>
        </div>
    }
}
