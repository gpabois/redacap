//! Interrupteur DSFR (`fr-toggle`) : bascule on/off.

use leptos::prelude::*;

/// Interrupteur on/off avec libellé.
#[component]
pub fn Toggle(
    label: &'static str,
    #[prop(into)] checked: Signal<bool>,
    on_toggle: impl Fn(bool) + 'static,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! {
        <label class=format!("{class} flex items-center gap-3 text-sm cursor-pointer disabled:opacity-50")>
            <span
                role="switch"
                aria-checked=move || checked.get().to_string()
                class=move || format!(
                    "relative inline-flex h-6 w-11 shrink-0 rounded-full transition-colors {}",
                    if checked.get() { "bg-blue-france" } else { "bg-gray-400" },
                )
                on:click=move |_| {
                    if !disabled {
                        on_toggle(!checked.get_untracked());
                    }
                }
            >
                <span
                    class=move || format!(
                        "absolute top-0.5 size-5 rounded-full bg-white transition-transform {}",
                        if checked.get() { "translate-x-[1.375rem]" } else { "translate-x-0.5" },
                    )
                ></span>
            </span>
            {label}
        </label>
    }
}
