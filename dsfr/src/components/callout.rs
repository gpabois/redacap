//! Mise en avant DSFR (`fr-callout`) : encart pour souligner une
//! information clé, avec titre et action optionnels.

use leptos::prelude::*;

/// Encart de mise en avant d'une information clé.
#[component]
pub fn Callout(
    #[prop(optional)] title: Option<&'static str>,
    #[prop(optional)] icon: Option<&'static str>,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("{class} bg-gray-100 dark:bg-gray-800 border-l-4 border-blue-france p-6")>
            {title.map(|title| view! {
                <p class="text-xl font-bold mb-2">
                    {icon.map(|icon| view! { <span class="mr-2">{icon}</span> })}
                    {title}
                </p>
            })}
            <div class="text-base text-gray-800 dark:text-gray-200">
                {children()}
            </div>
        </div>
    }
}
