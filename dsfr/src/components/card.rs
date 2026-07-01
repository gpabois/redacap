//! Carte DSFR (`fr-card`) : contenu structuré (titre, description, image,
//! liens) dans un encart.

use leptos::prelude::*;

/// Carte de présentation d'un contenu, avec titre, description et lien
/// optionnels.
#[component]
pub fn Card(
    title: &'static str,
    #[prop(optional)] description: Option<&'static str>,
    #[prop(optional)] href: Option<String>,
    #[prop(optional)] image_src: Option<String>,
    #[prop(optional)] class: &'static str,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class=format!("{class} flex flex-col border border-gray-300 overflow-hidden")>
            {image_src.map(|src| view! {
                <img src=src alt="" class="w-full h-40 object-cover" />
            })}
            <div class="flex flex-col gap-2 p-4">
                <h3 class="text-lg font-bold text-gray-900">
                    {match href.clone() {
                        Some(href) => view! { <a href=href class="hover:text-blue-france">{title}</a> }.into_any(),
                        None => view! { <span>{title}</span> }.into_any(),
                    }}
                </h3>
                {description.map(|description| view! { <p class="text-sm text-gray-700">{description}</p> })}
                {children.map(|children| view! { <div class="mt-2">{children()}</div> })}
            </div>
        </div>
    }
}
