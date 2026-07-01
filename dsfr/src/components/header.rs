//! En-tête DSFR (`fr-header`).

use leptos::prelude::*;

/// En-tête de page DSFR avec bloc Marianne et zone d'outils.
///
/// Les `children` sont rendus dans la zone outils (droite de l'en-tête).
///
/// # Structure
/// ```html
/// <header>
///   [Tricolore] République Française | <service_title>   [children →]
/// </header>
/// ```
#[component]
pub fn Header(
    /// Nom du service affiché à côté du logo Marianne.
    #[prop(into)]
    service_title: String,
    /// Accroche courte sous le nom du service.
    #[prop(optional, into)]
    service_tagline: Option<String>,
    /// Contenu de la zone outils (boutons, liens…) placé à droite.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    view! {
        <header
            role="banner"
            class="fr-header bg-white border-b border-gray-300 shadow-sm"
        >
            <div class="fr-header__body">
                <div class="max-w-screen-2xl mx-auto px-4 sm:px-6">
                    <div class="fr-header__body-row flex items-center justify-between min-h-14 gap-4 py-2">

                        // ── Bloc marque ──────────────────────────────────────
                        <div class="fr-header__brand flex items-center gap-3 shrink-0">
                            // Logo + service
                            <div class="fr-header__brand-top flex items-center gap-3">
                                <p class="fr-logo text-xs font-bold leading-tight uppercase tracking-wide text-gray-800 whitespace-pre">
                                    "République\nFrançaise"
                                </p>

                                // Séparateur
                                <div class="w-px h-8 bg-gray-300 shrink-0"/>

                                // Nom + accroche du service
                                <div class="fr-header__service">
                                    <span class="fr-header__service-title block text-sm font-bold text-[#000091] leading-tight">
                                        {service_title}
                                    </span>
                                    {service_tagline.map(|t| view! {
                                        <p class="fr-header__service-tagline text-xs text-gray-500 leading-tight mt-0.5">
                                            {t}
                                        </p>
                                    })}
                                </div>
                            </div>
                        </div>

                        // ── Zone outils ──────────────────────────────────────
                        {children.map(|c| view! {
                            <div class="fr-header__tools flex items-center gap-2 ml-auto flex-wrap">
                                {c()}
                            </div>
                        })}
                    </div>
                </div>
            </div>
        </header>
    }
}
