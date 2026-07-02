//! En-tête DSFR de l'éditeur d'acte légal.
//!
//! Affiche le [`dsfr::Header`] avec deux zones de boutons :
//! - Les **actions racine** (Visa, Considérant, Sur, Titre, Article, Annexe),
//!   toujours visibles.
//! - Le **portail contextuel** : boutons poussés par les composants enfants dans
//!   [`super::context::EditorContext::portal_actions`].
//! - Le **bouton agent IA** : affiché uniquement si `has_agent` est `true`.

use leptos::prelude::*;

use dsfr::{Button, ButtonGroup, ButtonVariant, Header, SubHeader};

use crate::{NodeKind, NodeSpec};
use crate::traits::node::{BodyRead, BodyWrite};
use super::context::expect_editor_context;
use super::widgets::FormatToolbar;

/// En-tête de l'éditeur : bloc Marianne + boutons d'action + portail contextuel.
#[component]
pub fn EditorHeader(
    /// Afficher le bouton de bascule du panneau agent IA.
    #[prop(optional)]
    has_agent: bool,
    /// Signal d'état du panneau agent (ouvert / fermé).
    agent_panel_open: Signal<bool>,
    /// Callback appelé pour basculer le panneau agent.
    on_toggle_agent: Callback<()>,
) -> impl IntoView {
    let ctx = expect_editor_context();
    let root = ctx.body.with_untracked(|b| b.root());

    let has_sur = move || {
        ctx.body.with(|b| {
            b.children_of(root)
                .iter()
                .any(|&id| b.kind_of(id) == NodeKind::Sur)
        })
    };

    let portal_actions = move || ctx.portal_actions.get();

    view! {
        <Header service_title="Redac'Ap" service_tagline="Éditeur d'arrêté préfectoral".to_string()>
            // ── Actions racine ────────────────────────────────────────────
            <SubHeader slot>
                <ButtonGroup class="divide-x">
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| { let _ = b.append_node(root, NodeSpec::Visa); });
                        }
                    >
                        "+ Visa"
                    </Button>
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| { let _ = b.append_node(root, NodeSpec::Considerant); });
                        }
                    >
                        "+ Considérant"
                    </Button>
                    <Show when=move || !has_sur()>
                        <Button
                            variant=ButtonVariant::TertiaryNoOutline
                            size=dsfr::Size::Sm
                            on_click=move |_| {
                                ctx.body.update(|b| { let _ = b.append_node(root, NodeSpec::Sur); });
                            }
                        >
                            "+ Sur"
                        </Button>
                    </Show>
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| {
                                let _ = b.append_node(root, NodeSpec::Article(Default::default()));
                            });
                        }
                    >
                        "+ Article"
                    </Button>
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| {
                                let _ = b.append_node(root, NodeSpec::Titre(Default::default()));
                            });
                        }
                    >
                        "+ Titre"
                    </Button>
                    <Button
                        variant=ButtonVariant::TertiaryNoOutline
                        size=dsfr::Size::Sm
                        on_click=move |_| {
                            ctx.body.update(|b| {
                                let _ = b.append_node(root, NodeSpec::Annexe(Default::default()));
                            });
                        }
                    >
                        "+ Annexe"
                    </Button>
                    // ── Bascule panneau agent ─────────────────────────────────────
                    {has_agent.then(|| view! {
                        <Button
                            variant=ButtonVariant::TertiaryNoOutline
                            size=dsfr::Size::Sm
                            on_click=move |_| on_toggle_agent.run(())
                        >
                            {move || if agent_panel_open.get() { "Masquer l'agent" } else { "Agent IA" }}
                        </Button>
                    })}
                </ButtonGroup>

                // ── Outils de mise en forme (nœud contenant un span) ──────
                <Show when=move || ctx.content_focus.get()>
                    <span class="inline-flex items-center align-middle gap-2 ml-2">
                        <span class="w-px h-6 bg-gray-300 inline-block"/>
                        <FormatToolbar/>
                    </span>
                </Show>

            </SubHeader>



            // ── Portail contextuel ────────────────────────────────────────
            <Show when=move || !portal_actions().is_empty()>
                <div class="w-px h-6 bg-gray-300 shrink-0"/>
                <ButtonGroup>
                    <For
                        each=portal_actions
                        key=|a| a.label.clone()
                        children=|action| {
                            let on_click = action.on_click.clone();
                            view! {
                                <Button
                                    variant=action.variant
                                    size=dsfr::Size::Sm
                                    on_click=move |_| (on_click)()
                                >
                                    {action.label}
                                </Button>
                            }
                        }
                    />
                </ButtonGroup>
            </Show>

        </Header>
    }
}
