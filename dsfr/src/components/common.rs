//! Tokens partagés par les composants DSFR : sévérités sémantiques et tailles.

/// Sévérité sémantique utilisée par [`crate::Alert`], [`crate::Badge`],
/// [`crate::Tag`] et [`crate::Notice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Success,
    Warning,
    Error,
}

impl Severity {
    /// Classes Tailwind pour le texte et les icônes.
    pub fn text_class(self) -> &'static str {
        match self {
            Severity::Info => "text-info",
            Severity::Success => "text-success",
            Severity::Warning => "text-warning",
            Severity::Error => "text-error",
        }
    }

    /// Classes Tailwind pour le fond.
    pub fn bg_class(self) -> &'static str {
        match self {
            Severity::Info => "bg-info-bg dark:bg-info/15",
            Severity::Success => "bg-success-bg dark:bg-success/15",
            Severity::Warning => "bg-warning-bg dark:bg-warning/15",
            Severity::Error => "bg-error-bg dark:bg-error/15",
        }
    }

    /// Classes Tailwind pour une bordure (alertes, notices).
    pub fn border_class(self) -> &'static str {
        match self {
            Severity::Info => "border-info",
            Severity::Success => "border-success",
            Severity::Warning => "border-warning",
            Severity::Error => "border-error",
        }
    }

    /// Libellé par défaut en français, utilisé quand aucun titre explicite
    /// n'est fourni par l'appelant.
    pub fn default_label(self) -> &'static str {
        match self {
            Severity::Info => "Information",
            Severity::Success => "Succès",
            Severity::Warning => "Avertissement",
            Severity::Error => "Erreur",
        }
    }
}

/// Taille d'un composant DSFR (bouton, badge, tag...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    Sm,
    #[default]
    Md,
    Lg,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_classes_are_pairwise_distinct() {
        let severities = [
            Severity::Info,
            Severity::Success,
            Severity::Warning,
            Severity::Error,
        ];
        for (i, a) in severities.iter().enumerate() {
            for b in &severities[i + 1..] {
                assert_ne!(a.text_class(), b.text_class());
                assert_ne!(a.bg_class(), b.bg_class());
                assert_ne!(a.border_class(), b.border_class());
            }
        }
    }

    #[test]
    fn size_defaults_to_md() {
        assert_eq!(Size::default(), Size::Md);
    }
}
