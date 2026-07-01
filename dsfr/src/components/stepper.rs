//! Étapes DSFR (`fr-stepper`) : suivi de progression d'un parcours.

use leptos::prelude::*;

/// Indicateur de progression dans un parcours en plusieurs étapes
/// (ex : workflow de validation d'un arrêté).
#[component]
pub fn Stepper(
    current_step: usize,
    total_steps: usize,
    title: &'static str,
    #[prop(optional)] next_title: Option<&'static str>,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let total_steps = total_steps.max(1);
    let progress = (current_step.min(total_steps) * 100) / total_steps;
    view! {
        <div class=format!("{class} flex flex-col gap-2")>
            <p class="text-sm text-gray-600">
                "Étape " {current_step} " sur " {total_steps}
            </p>
            <h2 class="text-xl font-bold text-gray-900">{title}</h2>
            <div class="h-1 w-full bg-gray-300 rounded-full overflow-hidden">
                <div class="h-full bg-blue-france" style=format!("width: {progress}%")></div>
            </div>
            {next_title.map(|next_title| view! {
                <p class="text-sm text-gray-600">
                    "Étape suivante : " {next_title}
                </p>
            })}
        </div>
    }
}
