//! Dérivation déterministe de l'affichage de présence (initiale, couleur)
//! d'un utilisateur connecté à une salle d'édition collaborative (voir
//! [`super::state::EditorRoom`]) : ces valeurs sont calculées côté serveur
//! et transmises telles quelles sur le fil (voir
//! [`super::protocol::PresenceUser`]), de sorte que tous les pairs affichent
//! la même pastille pour un même utilisateur.

use shared::id::ID;

/// Réduit un nom affiché à son initiale capitalisée, pour la pastille de
/// présence (même règle que `app::auth::display_initial`, dupliquée ici :
/// `server` ne peut pas dépendre d'`app`, qui dépend déjà de lui).
pub fn display_initial(display_name: &str) -> String {
    display_name
        .chars()
        .next()
        .map(|letter| letter.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Couleur HSL déterministe associée à un identifiant utilisateur : un même
/// utilisateur affiche toujours la même couleur de pastille, sans registre
/// partagé ni coordination entre pairs.
pub fn color_for_id(id: &ID) -> String {
    let hash = id.as_bytes().iter().fold(0u32, |acc, &byte| {
        acc.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    format!("hsl({}, 65%, 45%)", hash % 360)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_initial_capitalizes_first_letter() {
        assert_eq!(display_initial("alice"), "A");
        assert_eq!(display_initial("Élise"), "É");
        assert_eq!(display_initial(""), "?");
    }

    #[test]
    fn color_for_id_is_deterministic() {
        let id = shared::id::generate_id();
        assert_eq!(color_for_id(&id), color_for_id(&id));
    }
}
