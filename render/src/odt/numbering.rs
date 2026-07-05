//! Numérotation des subdivisions et correspondance avec les styles de
//! liste déclarés statiquement dans `styles.xml` (voir [`super::styles`]).

use content::ListMarker;

use crate::odt::style_names as s;

const ROMAN_TABLE: &[(u32, &str)] = &[
    (1000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];

/// Représentation en chiffres romains majuscules d'un nombre (1-based).
/// Renvoie l'écriture arabe pour `0`, qui ne devrait pas survenir pour une
/// subdivision numérotée.
pub(crate) fn to_roman(mut n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut out = String::new();
    for &(value, symbol) in ROMAN_TABLE {
        while n >= value {
            out.push_str(symbol);
            n -= value;
        }
    }
    out
}

/// Nom du style de liste ODF associé à un marqueur.
pub(crate) fn list_style_name(marker: ListMarker) -> &'static str {
    match marker {
        ListMarker::Disc => s::LIST_DISC,
        ListMarker::Circle => s::LIST_CIRCLE,
        ListMarker::Square => s::LIST_SQUARE,
        ListMarker::Decimal => s::LIST_DECIMAL,
        ListMarker::LowerAlpha => s::LIST_LOWER_ALPHA,
        ListMarker::UpperAlpha => s::LIST_UPPER_ALPHA,
        ListMarker::LowerRoman => s::LIST_LOWER_ROMAN,
        ListMarker::UpperRoman => s::LIST_UPPER_ROMAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_roman() {
        assert_eq!(to_roman(1), "I");
        assert_eq!(to_roman(4), "IV");
        assert_eq!(to_roman(9), "IX");
        assert_eq!(to_roman(14), "XIV");
        assert_eq!(to_roman(49), "XLIX");
        assert_eq!(to_roman(2024), "MMXXIV");
    }
}
