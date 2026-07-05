use leptos::prelude::*;
use leptos::{IntoView, component, view};

#[component]
pub fn BlocMarianne(children: Children, #[prop(optional)] class: &'static str) -> impl IntoView {
    view! {
        <div class={class}>
            <p class="fr-logo">
                {children()}
            </p>
        </div>
    }
}

#[component]
pub fn BlocMarianneInline<S: ToString>(
    autorite: S,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let lines = autorite
        .to_string()
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            view! {
                // On n'affiche le <br/> que si ce n'est pas la première ligne
                {(index > 0).then(|| view! { <br/> })}
                <span>{line.to_owned()}</span>
            }
        })
        .collect::<Vec<_>>();
    view! {
        <div class={class}>
            <p class="fr-logo">
                {lines}
            </p>
        </div>
    }
}
