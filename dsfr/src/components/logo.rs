//! Emblème de l'éditeur d'arrêtés préfectoraux — « L'Arrêté scellé ».
//!
//! Deux composants :
//! * [`Logo`]        — emblème principal (le document frappé du sceau RF).
//! * [`LogoFavicon`] — version « pleine » simplifiée, lisible jusqu'à 16 px.
//!
//! Palette : bleu République `#000091`, papier `#F7F6F1`, filet tricolore `#E1000F`.

use leptos::prelude::*;

/// Bleu République (bleu Marianne).
pub const BLEU_REPUBLIQUE: &str = "#000091";
/// Fond « papier » de l'arrêté.
pub const PAPIER: &str = "#F7F6F1";
/// Rouge du filet tricolore.
pub const ROUGE: &str = "#E1000F";

/// Teinte d'encre appliquée au logo.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Teinte {
    /// Bleu + filet tricolore rouge (usage principal).
    #[default]
    Couleur,
    /// Aplat bleu République unique.
    Bleu,
    /// Aplat noir encre (documents monochromes).
    Encre,
}

impl Teinte {
    /// Couleur des traits principaux.
    fn trait_(self) -> &'static str {
        match self {
            Teinte::Couleur | Teinte::Bleu => BLEU_REPUBLIQUE,
            Teinte::Encre => "#101534",
        }
    }
    /// Couleur du filet tricolore (masqué hors couleur).
    fn filet(self) -> &'static str {
        match self {
            Teinte::Couleur => ROUGE,
            _ => "none",
        }
    }
}

/// Emblème principal « L'Arrêté scellé ».
///
/// ```ignore
/// view! { <Logo size=64 /> }
/// view! { <Logo size=120 teinte=Teinte::Encre /> }
/// ```
#[component]
pub fn Logo(
    /// Côté du carré de rendu, en pixels. Défaut : 120.
    #[prop(default = 120)]
    size: u32,
    /// Teinte appliquée. Défaut : [`Teinte::Couleur`].
    #[prop(default = Teinte::Couleur)]
    teinte: Teinte,
    /// Classes Tailwind supplémentaires (ex. `w-8 h-8` pour surcharger
    /// `size` via CSS, notamment en variante responsive).
    #[prop(optional)]
    class: &'static str,
) -> impl IntoView {
    let trait_ = teinte.trait_();
    let filet = teinte.filet();
    view! {
        <svg
            width={size}
            height={size}
            viewBox="0 0 120 120"
            xmlns="http://www.w3.org/2000/svg"
            role="img"
            aria-label="Éditeur d'arrêtés préfectoraux"
            class=class
        >
            // Feuille de l'arrêté
            <rect x="33" y="20" width="54" height="80" rx="3" fill=PAPIER stroke=trait_ stroke-width="2.2"/>
            // Bandeau d'en-tête
            <rect x="33" y="20" width="54" height="15" rx="3" fill=trait_/>
            <rect x="33" y="31" width="54" height="4" fill=trait_/>
            <rect x="41" y="27" width="18" height="2.5" rx="1.25" fill="#ffffff" opacity="0.9"/>
            // Corps de texte
            <rect x="41" y="45" width="38" height="3" rx="1.5" fill=trait_/>
            <rect x="41" y="53" width="38" height="3" rx="1.5" fill=trait_/>
            <rect x="41" y="61" width="26" height="3" rx="1.5" fill=trait_/>
            // Sceau RF
            <circle cx="72" cy="86" r="16" fill=trait_/>
            <circle cx="72" cy="86" r="12.5" fill="none" stroke="#ffffff" stroke-width="1"/>
            <text x="72" y="91" text-anchor="middle" font-family="Georgia, 'Times New Roman', serif" font-weight="700" font-size="13" fill="#ffffff">"RF"</text>
            // Filet tricolore
            <rect x="60.6" y="96" width="7" height="3.4" fill=filet/>
        </svg>
    }
}
