//! Liens d'évitement DSFR (`fr-skiplinks`) : raccourcis d'accessibilité
//! invisibles tant qu'ils ne reçoivent pas le focus clavier.

use leptos::prelude::*;

/// Lien d'évitement (cible une ancre, ex: `"#contenu"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipLink {
    pub label: String,
    pub anchor: String,
}

impl SkipLink {
    pub fn new(label: impl Into<String>, anchor: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            anchor: anchor.into(),
        }
    }
}

/// Liste de liens d'évitement, masqués jusqu'à recevoir le focus clavier
/// (navigation au tabulateur).
#[component]
pub fn SkipLinks(links: Vec<SkipLink>, #[prop(optional)] class: &'static str) -> impl IntoView {
    view! {
        <nav aria-label="Accès rapide" class=format!("{class} relative")>
            <ul class="list-none">
                {links.into_iter().map(|link| view! {
                    <li>
                        <a
                            href=format!("#{}", link.anchor)
                            class="sr-only focus:not-sr-only focus:absolute focus:top-0 focus:left-0 focus:z-50 focus:bg-blue-france focus:text-white focus:px-4 focus:py-2"
                        >
                            {link.label}
                        </a>
                    </li>
                }).collect::<Vec<_>>()}
            </ul>
        </nav>
    }
}
