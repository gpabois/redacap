//! Champ de saisie DSFR (`fr-input`) : libellé, texte d'aide et état
//! d'erreur.

use leptos::ev::Event;
use leptos::prelude::*;

/// Champ de saisie texte avec libellé et gestion d'erreur.
#[component]
pub fn Input(
    label: &'static str,
    #[prop(into)] value: Signal<String>,
    on_input: impl Fn(String) + 'static,
    #[prop(optional, default = "text")] r#type: &'static str,
    #[prop(optional)] hint: Option<&'static str>,
    #[prop(optional)] error: Option<String>,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let has_error = error.is_some();
    let input_border = if has_error {
        "shadow-[inset_0_0_0_2px] shadow-error"
    } else {
        "shadow-[inset_0_0_0_1px] shadow-gray-400 focus:shadow-[inset_0_0_0_2px] focus:shadow-blue-france"
    };
    view! {
        <div class=format!("{class} flex flex-col gap-1")>
            <label class="text-sm font-bold text-gray-900">{label}</label>
            {hint.map(|hint| view! { <span class="text-sm text-gray-600">{hint}</span> })}
            <input
                type=r#type
                class=format!("{input_border} bg-gray-100 px-3 py-2 outline-none disabled:opacity-50")
                prop:value=move || value.get()
                disabled=disabled
                on:input=move |ev: Event| on_input(event_target_value(&ev))
            />
            {error.map(|error| view! { <span class="text-sm text-error font-bold">{error}</span> })}
        </div>
    }
}
