/// Composants Leptos pour l'affichage et la saisie des commentaires
/// (review) d'un projet d'acte légal.
///
/// Les commentaires sont ancrés à une [`crate::cursor::Selection`] et
/// peuvent être résolus ou recevoir des réponses arborescentes.
use leptos::prelude::*;

use crate::traits::review::Comment;

/// Affiche un commentaire et ses réponses imbriquées.
#[component]
pub fn CommentThread(comment: Comment) -> impl IntoView {
    // TODO: afficher le texte, l'auteur, le bouton "résoudre" et les réponses
    let _ = comment;
    view! { <div class="comment-thread"></div> }
}

/// Panneau listant tous les commentaires non résolus du projet.
#[component]
pub fn ReviewPanel() -> impl IntoView {
    // TODO: intégrer avec le contexte du projet
    view! { <aside class="review-panel"></aside> }
}
