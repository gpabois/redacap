//! Citation DSFR (`fr-quote`) : citation avec auteur et source optionnels.

use leptos::prelude::*;

/// Citation mise en forme, avec auteur et source optionnels.
#[component]
pub fn Quote(
    #[prop(optional)] author: Option<&'static str>,
    #[prop(optional)] source: Option<&'static str>,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <figure class=format!("{class} border-l-4 border-blue-france pl-6")>
            <blockquote class="text-xl italic text-gray-900">
                {children()}
            </blockquote>
            {(author.is_some() || source.is_some()).then(|| view! {
                <figcaption class="mt-2 text-sm text-gray-600">
                    {author.map(|author| view! { <span class="font-bold">{author}</span> })}
                    {source.map(|source| view! { <cite class="block">{source}</cite> })}
                </figcaption>
            })}
        </figure>
    }
}
