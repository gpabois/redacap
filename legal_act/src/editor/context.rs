use std::sync::Arc;

use dsfr::ButtonVariant;
use leptos::prelude::*;

use crate::Body;
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
    /// Actions contextuelles affichées dans la zone portail de l'en-tête.
    /// Les composants enfants écrivent ici ; [`super::header::EditorHeader`] lit.
    pub portal_actions: RwSignal<Vec<PortalAction>>,
}

impl EditorContext {
    pub fn new(body: Body) -> Self {
        Self {
            body: RwSignal::new(body),
            selection: RwSignal::new(EditorSelection::default()),
            portal_actions: RwSignal::new(Vec::new()),
        }
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

pub fn provide_editor_context(body: impl Into<Body>) -> EditorContext {
    let ctx = EditorContext::new(body.into());
    provide_context(ctx);
    ctx
}

pub fn expect_editor_context() -> EditorContext {
    expect_context::<EditorContext>()
}
