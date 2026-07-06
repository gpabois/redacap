//! Tableau DSFR (`fr-table`) : en-têtes fournis, lignes du corps laissées
//! à la charge de l'appelant.

use leptos::prelude::*;

/// Tableau de données stylisé. Les lignes (`<tr>`) sont fournies par
/// l'appelant via `children`.
#[component]
pub fn Table(
    headers: Vec<&'static str>,
    #[prop(optional)] caption: Option<&'static str>,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("{class} overflow-x-auto")>
            <table class="w-full text-sm border-collapse">
                {caption.map(|caption| view! { <caption class="text-left font-bold mb-2">{caption}</caption> })}
                <thead>
                    <tr class="border-b-2 border-gray-900">
                        {headers.into_iter().map(|header| view! {
                            <th scope="col" class="text-left px-3 py-2 font-bold">{header}</th>
                        }).collect::<Vec<_>>()}
                    </tr>
                </thead>
                <tbody class="*:border-b *:border-gray-300 dark:border-gray-700 *:odd:bg-gray-50 dark:bg-gray-800">
                    {children()}
                </tbody>
            </table>
        </div>
    }
}
