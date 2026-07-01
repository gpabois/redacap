//! Fil d'Ariane DSFR (`fr-breadcrumb`).

use leptos::prelude::*;

/// Maillon du fil d'Ariane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreadcrumbItem {
    pub label: String,
    pub href: Option<String>,
}

impl BreadcrumbItem {
    /// Maillon cliquable, menant vers `href`.
    pub fn link(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: Some(href.into()),
        }
    }

    /// Maillon final, page courante (non cliquable).
    pub fn current(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: None,
        }
    }
}

/// Fil d'Ariane indiquant la position dans l'arborescence du site.
#[component]
pub fn Breadcrumb(
    items: Vec<BreadcrumbItem>,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let count = items.len();
    view! {
        <nav aria-label="vous êtes ici :" class=format!("{class} text-sm")>
            <ol class="flex flex-wrap items-center gap-1 list-none">
                {items.into_iter().enumerate().map(|(i, item)| {
                    let is_last = i == count - 1;
                    view! {
                        <li class="flex items-center gap-1">
                            {match item.href {
                                Some(href) if !is_last => view! {
                                    <a href=href class="text-blue-france hover:underline">{item.label}</a>
                                }.into_any(),
                                _ => view! {
                                    <span aria-current="page" class="text-gray-600">{item.label}</span>
                                }.into_any(),
                            }}
                            {(!is_last).then(|| view! { <span class="text-gray-400">"›"</span> })}
                        </li>
                    }
                }).collect::<Vec<_>>()}
            </ol>
        </nav>
    }
}
