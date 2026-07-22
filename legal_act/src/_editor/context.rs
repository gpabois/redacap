use std::collections::HashSet;
use std::sync::Arc;

use dsfr::ButtonVariant;
use leptos::prelude::*;

use super::state::{EditorSelection, PendingComment};
use crate::traits::node::BodyAccess;
use crate::traits::review::ReviewAccess;
use crate::{Body, NodeId, Review};

/// Action contextuelle injectée dans la zone portail de l'en-tête.
///
/// Un composant enfant de l'éditeur peut pousser des `PortalAction` dans
/// `EditorContext::portal_actions` pour y faire apparaître des boutons
/// spécifiques à son nœud (ex. : « + Chapitre » quand un Titre est actif).
#[derive(Clone)]
pub struct PortalAction {
    pub label: String,
    pub variant: ButtonVariant,
    pub on_click: Arc<dyn Fn() + Send + Sync>,
}

impl PortalAction {
    pub fn new(label: impl Into<String>, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            label: label.into(),
            variant: ButtonVariant::Secondary,
            on_click: Arc::new(on_click),
        }
    }

    /// Variante principale (fond bleu).
    pub fn primary(mut self) -> Self {
        self.variant = ButtonVariant::Primary;
        self
    }
}

/// Contexte partagé par tous les composants de l'éditeur.
/// Fourni via [`provide_editor_context`] au niveau de [`super::component::LegalActEditor`].
#[derive(Clone, Copy)]
pub struct EditorContext {
    pub body: RwSignal<Body>,
    /// Commentaires et notes de travail du projet (voir [`crate::Review`]),
    /// possédés par la page hôte au même titre que [`Self::body`].
    pub reviews: RwSignal<Review>,
    pub selection: RwSignal<EditorSelection>,
    /// Nœud actuellement « ciblé » par l'utilisateur pour l'agent IA (bouton
    /// « Cibler » sur un visa/considérant/nœud structurel), `None` si aucun.
    /// Contrairement à [`Self::selection`] (curseur de texte fin, par
    /// caractère), c'est une désignation grossière d'un nœud entier :
    /// c'est elle qui est transmise au serveur (voir `app::ws::RoomHandle::
    /// set_selection`) pour que l'agent puisse viser ce nœud via le mot-clé
    /// `"selection"`, sans jamais exposer d'identifiant technique à
    /// l'utilisateur. Voir [`Self::toggle_agent_target`].
    pub agent_target: RwSignal<Option<NodeId>>,
    /// Actions contextuelles affichées dans la zone portail de l'en-tête.
    /// Les composants enfants écrivent ici ; [`super::header::EditorHeader`] lit.
    pub portal_actions: RwSignal<Vec<PortalAction>>,
    /// Vrai tant qu'un nœud de contenu pouvant contenir un `Span` (Paragraphe,
    /// élément de liste, cellule de tableau…) est en cours d'édition, c'est-à-
    /// dire qu'un [`super::widgets::RichEditableDiv`] a le focus. Piloté par
    /// ce dernier ; lu par [`super::header::EditorHeader`] pour afficher les
    /// outils de mise en forme (Barré/Gras/Italique) dans le sous-en-tête et
    /// les masquer dès qu'aucun nœud n'est focus.
    pub content_focus: RwSignal<bool>,
    /// Identifiant du nœud de contenu dont le [`super::widgets::RichEditableDiv`]
    /// a actuellement le focus clavier. `None` si aucun. Utilisé par
    /// [`super::header::ContentToolbar`] pour afficher des boutons contextuels.
    pub content_focus_node: RwSignal<Option<NodeId>>,
    /// Requête de focus programmatique : `Some((node_id, at_end))` demande au
    /// [`super::widgets::RichEditableDiv`] dont `focus_node_id == node_id` de
    /// prendre le focus. Quand `at_end` est vrai, le curseur est placé à la
    /// fin du contenu (utile après une fusion).
    pub content_focus_request: RwSignal<Option<(NodeId, bool)>>,
    /// Identité affichée de l'utilisateur courant, utilisée comme auteur des
    /// commentaires qu'il crée et comme clé de permission (suppression
    /// réservée à l'auteur). `None` tant que l'utilisateur n'est pas
    /// authentifié : aucune création de commentaire n'est alors possible.
    pub current_user: RwSignal<Option<String>>,
    /// `true` si l'utilisateur courant a les droits d'édition sur ce projet.
    /// Un commentaire peut être résolu par son auteur ou par un rédacteur
    /// (voir [`crate::traits::review::ReviewAccess::try_resolve_comment`]).
    pub can_edit: RwSignal<bool>,
    /// Amorce du commentaire en cours de composition dans le panneau
    /// latéral (voir [`super::review::ReviewPanel`]) ; `None` si le
    /// compositeur est fermé.
    pub pending_comment: RwSignal<Option<PendingComment>>,
    /// `true` si le panneau latéral (agent IA / commentaires / paramètres)
    /// est affiché.
    pub side_panel_open: RwSignal<bool>,
    /// Onglet actif du panneau latéral (0 = Marie, 1 = Commentaires,
    /// 2 = Paramètres).
    pub side_panel_tab: RwSignal<usize>,
}

impl EditorContext {
    pub fn new(
        body: RwSignal<Body>,
        reviews: RwSignal<Review>,
        current_user: Option<String>,
        can_edit: bool,
    ) -> Self {
        Self {
            body,
            reviews,
            selection: RwSignal::new(EditorSelection::default()),
            agent_target: RwSignal::new(None),
            portal_actions: RwSignal::new(Vec::new()),
            content_focus: RwSignal::new(false),
            content_focus_node: RwSignal::new(None),
            content_focus_request: RwSignal::new(None),
            current_user: RwSignal::new(current_user),
            can_edit: RwSignal::new(can_edit),
            pending_comment: RwSignal::new(None),
            side_panel_open: RwSignal::new(true),
            side_panel_tab: RwSignal::new(0),
        }
    }

    /// Demande le focus programmatique sur `node_id`.
    /// Si `at_end` est vrai, le curseur est placé à la fin du contenu.
    pub fn request_focus(&self, node_id: NodeId, at_end: bool) {
        self.content_focus_request.set(Some((node_id, at_end)));
    }

    /// Cible `node_id` pour l'agent IA, ou retire la cible si `node_id`
    /// était déjà ciblé (bascule).
    pub fn toggle_agent_target(&self, node_id: NodeId) {
        self.agent_target.update(|t| {
            *t = if *t == Some(node_id) {
                None
            } else {
                Some(node_id)
            }
        });
    }

    /// Remplace les actions du portail par la liste fournie.
    pub fn set_portal_actions(&self, actions: Vec<PortalAction>) {
        self.portal_actions.set(actions);
    }

    /// Vide les actions du portail.
    pub fn clear_portal_actions(&self) {
        self.portal_actions.set(Vec::new());
    }

    /// Supprime `node_id` (et son sous-arbre) du corps, puis supprime tout
    /// commentaire dont la sélection couvre une feuille de ce sous-arbre :
    /// un commentaire ne doit jamais survivre à la disparition de la
    /// section qu'il annote (voir `Claude.md`). Utilise
    /// [`crate::traits::review::ReviewAccess::delete_comment_thread`] pour
    /// emporter aussi les réponses du commentaire supprimé.
    pub fn remove_node_with_comments(&self, node_id: NodeId) {
        let orphaned = self.body.with_untracked(|b| {
            let removed = subtree_ids(b, node_id);
            self.reviews.with_untracked(|r| {
                r.comments()
                    .into_iter()
                    .filter(|c| {
                        c.selection.is_some_and(|selection| {
                            selection
                                .covered_leafs(b)
                                .iter()
                                .any(|leaf| removed.contains(leaf))
                        })
                    })
                    .map(|c| c.id)
                    .collect::<Vec<_>>()
            })
        });

        self.body.update(|b| {
            let _ = b.remove_node(node_id);
        });

        if !orphaned.is_empty() {
            self.reviews.update(|r| {
                for id in orphaned {
                    let _ = r.delete_comment_thread(id);
                }
            });
        }
    }
}

/// `node_id` et l'ensemble de ses descendants, utilisé par
/// [`EditorContext::remove_node_with_comments`] pour déterminer les
/// commentaires devenus orphelins.
fn subtree_ids(body: &impl BodyAccess, node_id: NodeId) -> HashSet<NodeId> {
    let mut ids = HashSet::new();
    let mut stack = vec![node_id];
    while let Some(current) = stack.pop() {
        if ids.insert(current) {
            stack.extend(body.children_of(current));
        }
    }
    ids
}

/// `body` reste possédé par l'appelant (page hôte) : c'est ce qui permet à
/// un client externe — par exemple le module `app::ws` qui synchronise le
/// document avec le salon websocket du crate `server` — de continuer à
/// écrire dans le même signal après le montage de [`super::component::LegalActEditor`].
pub fn provide_editor_context(
    body: RwSignal<Body>,
    reviews: RwSignal<Review>,
    current_user: Option<String>,
    can_edit: bool,
) -> EditorContext {
    let ctx = EditorContext::new(body, reviews, current_user, can_edit);
    provide_context(ctx);
    ctx
}

pub fn expect_editor_context() -> EditorContext {
    expect_context::<EditorContext>()
}
