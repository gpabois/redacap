use leptos::{IntoView, component, view};
use leptos::prelude::*;

#[component]
pub fn BlocMarianneInline(autorite: String, class: &'static str) -> impl IntoView {
    let lines = autorite
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