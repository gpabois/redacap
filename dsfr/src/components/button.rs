//! Bouton DSFR : actions principales, secondaires et tertiaires.

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::components::common::Size;

/// Variante visuelle d'un bouton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Tertiary,
    TertiaryNoOutline,
}

impl ButtonVariant {
    fn class(self) -> &'static str {
        match self {
            ButtonVariant::Primary => {
                "bg-blue-france text-white hover:bg-blue-france-hover active:bg-blue-france-active"
            }
            ButtonVariant::Secondary => {
                "bg-transparent text-blue-france shadow-[inset_0_0_0_1px] shadow-blue-france hover:bg-blue-france-975 dark:text-blue-france-925 dark:shadow-blue-france-925 dark:hover:bg-gray-800"
            }
            ButtonVariant::Tertiary => {
                "bg-transparent text-blue-france shadow-[inset_0_0_0_1px] shadow-gray-300 hover:bg-blue-france-975 dark:text-blue-france-925 dark:shadow-gray-700 dark:hover:bg-gray-800"
            }
            ButtonVariant::TertiaryNoOutline => {
                "bg-transparent text-blue-france hover:bg-gray-200 dark:text-blue-france-925 dark:hover:bg-gray-800"
            }
        }
    }
}

fn size_class(size: Size) -> &'static str {
    match size {
        Size::Sm => "text-sm px-3 py-1.5 short:py-0.5",
        Size::Md => "text-base px-4 py-2",
        Size::Lg => "text-lg px-5 py-2.5",
    }
}

/// Bouton d'action DSFR (`fr-btn`).
#[component]
pub fn Button(
    #[prop(optional)] variant: ButtonVariant,
    #[prop(optional)] size: Size,
    #[prop(optional)] disabled: bool,
    #[prop(optional, default = "button")] r#type: &'static str,
    #[prop(optional)] class: &'static str,
    on_click: impl Fn(MouseEvent) + 'static,
    children: Children,
) -> impl IntoView {
    view! {
        <button
            type=r#type
            class=format!(
                "{} {} {class} inline-flex items-center justify-center font-bold cursor-pointer transition-colors disabled:cursor-not-allowed disabled:opacity-40",
                variant.class(),
                size_class(size),
            )
            disabled=disabled
            on:click=on_click
        >
            {children()}
        </button>
    }
}

/// Regroupement horizontal de boutons liés (`fr-btns-group`).
#[component]
pub fn ButtonGroup(#[prop(optional)] class: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class=format!("{class} inline-flex flex-wrap")>
            {children()}
        </div>
    }
}
