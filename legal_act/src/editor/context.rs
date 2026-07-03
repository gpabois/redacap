use std::sync::Arc;

use dsfr::ButtonVariant;
use leptos::prelude::*;

use crate::{Body, BodyNodeId};
use super::state::EditorSelection;

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
    pub fn new(
        label: impl Into<String>,
        on_click: impl Fn() + Send + Sync + 'static,
    ) -> Self {
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
    pub selection: RwSignal<EditorSelection>,
    /// Nœud actuellement « ciblé » par l'utilisateur pour l'agent IA (bouton
    /// « Cibler » sur un visa/considérant/nœud structurel), `None` si aucun.
    /// Contrairement à [`Self::selection`] (curseur de texte fin, par
    /// caractère), c'est une désignation grossière d'un nœud entier :
    /// c'est elle qui est transmise au serveur (voir `app::ws::RoomHandle::
    /// set_selection`) pour que l'agent puisse viser ce nœud via le mot-clé
    /// `"selection"`, sans jamais exposer d'identifiant technique à
    /// l'utilisateur. Voir [`Self::toggle_agent_target`].
    pub agent_target: RwSignal<Option<BodyNodeId>>,
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
    pub content_focus_node: RwSignal<Option<BodyNodeId>>,
    /// Requête de focus programmatique : `Some((node_id, at_end))` demande au
    /// [`super::widgets::RichEditableDiv`] dont `focus_node_id == node_id` de
    /// prendre le focus. Quand `at_end` est vrai, le curseur est placé à la
    /// fin du contenu (utile après une fusion).
    pub content_focus_request: RwSignal<Option<(BodyNodeId, bool)>>,
}

impl EditorContext {
    pub fn new(body: RwSignal<Body>) -> Self {
        Self {
            body,
            selection: RwSignal::new(EditorSelection::default()),
            agent_target: RwSignal::new(None),
            portal_actions: RwSignal::new(Vec::new()),
            content_focus: RwSignal::new(false),
            content_focus_node: RwSignal::new(None),
            content_focus_request: RwSignal::new(None),
        }
    }

    /// Demande le focus programmatique sur `node_id`.
    /// Si `at_end` est vrai, le curseur est placé à la fin du contenu.
    pub fn request_focus(&self, node_id: BodyNodeId, at_end: bool) {
        self.content_focus_request.set(Some((node_id, at_end)));
    }

    /// Cible `node_id` pour l'agent IA, ou retire la cible si `node_id`
    /// était déjà ciblé (bascule).
    pub fn toggle_agent_target(&self, node_id: BodyNodeId) {
        self.agent_target.update(|t| *t = if *t == Some(node_id) { None } else { Some(node_id) });
    }

    /// Remplace les actions du portail par la liste fournie.
    pub fn set_portal_actions(&self, actions: Vec<PortalAction>) {
        self.portal_actions.set(actions);
    }

    /// Vide les actions du portail.
    pub fn clear_portal_actions(&self) {
        self.portal_actions.set(Vec::new());
    }
}

/// `body` reste possédé par l'appelant (page hôte) : c'est ce qui permet à
/// un client externe — par exemple le module `app::ws` qui synchronise le
/// document avec le salon websocket du crate `server` — de continuer à
/// écrire dans le même signal après le montage de [`super::component::LegalActEditor`].
pub fn provide_editor_context(body: RwSignal<Body>) -> EditorContext {
    let ctx = EditorContext::new(body);
    provide_context(ctx);
    ctx
}

pub fn expect_editor_context() -> EditorContext {
    expect_context::<EditorContext>()
}
