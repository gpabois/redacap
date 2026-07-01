//! Sommaire DSFR (`fr-summary`) : table des matières d'une page longue.

use leptos::prelude::*;

/// Entrée du [`Summary`], pointant vers une ancre de la page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryItem {
    pub label: String,
    pub anchor: String,
}

impl SummaryItem {
    pub fn new(label: impl Into<String>, anchor: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            anchor: anchor.into(),
        }
    }
}

/// Table des matières d'une page longue, liste de liens vers des ancres.
#[component]
pub fn Summary(items: Vec<SummaryItem>, #[prop(optional)] class: &'static str) -> impl IntoView {
    view! {
        <nav aria-label="Sommaire" class=format!("{class} bg-gray-100 p-6")>
            <p class="font-bold text-gray-900 mb-2">"Sommaire"</p>
            <ol class="flex flex-col gap-1 list-decimal list-inside text-sm">
                {items.into_iter().map(|item| view! {
                    <li>
                        <a href=format!("#{}", item.anchor) class="text-blue-france hover:underline">
                            {item.label}
                        </a>
                    </li>
                }).collect::<Vec<_>>()}
            </ol>
        </nav>
    }
}
