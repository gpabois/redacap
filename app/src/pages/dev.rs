//! Page de démonstration des états du panneau agent IA.
//! Accessible en développement à la route `/dev/agent`.

use agent::{AgentPanel, InteractionRequest, InteractionResponse, PanelMessage, PanelQuestion};
use leptos::prelude::*;

// ── Scénarios ────────────────────────────────────────────────────────────────

fn messages_conversation() -> Vec<PanelMessage> {
    vec![
        PanelMessage::user("Complète les visas réglementaires"),
        PanelMessage::assistant(
            "J'ai besoin de connaître la rubrique ICPE principale. \
             Quel est le code rubrique de l'installation ?",
        ),
        PanelMessage::user("2760-1"),
        PanelMessage::assistant(
            "Merci. Je recherche les textes applicables à la rubrique 2760-1 \
             et je vais compléter les visas correspondants.",
        ),
        PanelMessage::user("Ajoute aussi le visa relatif au code de l'environnement"),
    ]
}

fn interaction_formulaire() -> InteractionRequest {
    InteractionRequest {
        prompt: "Pour compléter les métadonnées de l'installation, \
                 veuillez renseigner les informations suivantes :"
            .to_string(),
        questions: vec![
            PanelQuestion {
                id: "nom_exploitant".to_string(),
                label: "Nom de l'exploitant".to_string(),
                options: None,
            },
            PanelQuestion {
                id: "regime".to_string(),
                label: "Régime de l'installation".to_string(),
                options: Some(vec![
                    "Autorisation".to_string(),
                    "Enregistrement".to_string(),
                    "Déclaration".to_string(),
                ]),
            },
            PanelQuestion {
                id: "siret".to_string(),
                label: "Numéro SIRET".to_string(),
                options: None,
            },
        ],
    }
}

fn interaction_confirmation() -> InteractionRequest {
    InteractionRequest {
        prompt: "Avant de remplir la section « Considérants », \
                 confirmez les points suivants :"
            .to_string(),
        questions: vec![
            PanelQuestion {
                id: "rubrique_ok".to_string(),
                label: "La rubrique ICPE 2760-1 est bien celle de l'installation ?".to_string(),
                options: Some(vec!["Oui".to_string(), "Non".to_string()]),
            },
            PanelQuestion {
                id: "seuil".to_string(),
                label: "Seuil de classement retenu (en tonnes)".to_string(),
                options: None,
            },
        ],
    }
}

// ── Composant principal ───────────────────────────────────────────────────────

/// Page de démonstration présentant les différents états possibles
/// du [`AgentPanel`].
#[component]
pub fn PageDevAgentPanel() -> impl IntoView {
    view! {
        <div class="min-h-screen bg-stone-50 p-6">
            <h1 class="text-lg font-bold text-stone-800 mb-1">"Démo — Panneau agent IA"</h1>
            <p class="text-xs text-stone-500 mb-6">
                "Aperçu des différents états de la boucle agentique."
            </p>

            <div class="grid grid-cols-3 gap-4 mb-6">
                <Scenario titre="Panneau vide">
                    <ScenarioVide/>
                </Scenario>
                <Scenario titre="Agent en attente">
                    <ScenarioEnAttente/>
                </Scenario>
                <Scenario titre="Conversation en cours">
                    <ScenarioConversation/>
                </Scenario>
            </div>

            <div class="grid grid-cols-2 gap-4 mb-6">
                <Scenario titre="Formulaire — questions mixtes">
                    <ScenarioFormulaire/>
                </Scenario>
                <Scenario titre="Formulaire — avec confirmation">
                    <ScenarioConfirmation/>
                </Scenario>
            </div>

            <div class="grid grid-cols-1 gap-4">
                <Scenario titre="Conversation + formulaire (enchaînement)">
                    <ScenarioConversationPuisFormulaire/>
                </Scenario>
            </div>
        </div>
    }
}

// ── Conteneur d'un scénario ───────────────────────────────────────────────────

#[component]
fn Scenario(titre: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-1">
            <div class="text-xs font-semibold text-stone-500 uppercase tracking-wide px-1">
                {titre}
            </div>
            <div class="h-96 border border-stone-200 rounded shadow-sm overflow-hidden">
                {children()}
            </div>
        </div>
    }
}

// ── Scénario 1 : panneau vide ─────────────────────────────────────────────────

#[component]
fn ScenarioVide() -> impl IntoView {
    let messages = Signal::derive(|| Vec::<PanelMessage>::new());
    let pending = Signal::derive(|| false);
    view! {
        <AgentPanel
            messages=messages
            pending=pending
            on_send=|_| {}
        />
    }
}

// ── Scénario 2 : agent en attente ────────────────────────────────────────────

#[component]
fn ScenarioEnAttente() -> impl IntoView {
    let messages = Signal::derive(|| vec![PanelMessage::user("Rédige les visas de l'arrêté")]);
    let pending = Signal::derive(|| true);
    view! {
        <AgentPanel
            messages=messages
            pending=pending
            on_send=|_| {}
        />
    }
}

// ── Scénario 3 : conversation en cours ───────────────────────────────────────

#[component]
fn ScenarioConversation() -> impl IntoView {
    let messages = Signal::derive(messages_conversation);
    let pending = Signal::derive(|| false);
    view! {
        <AgentPanel
            messages=messages
            pending=pending
            on_send=|_| {}
        />
    }
}

// ── Scénario 4 : formulaire actif (texte libre + sélecteur) ──────────────────

#[component]
fn ScenarioFormulaire() -> impl IntoView {
    let messages = Signal::derive(|| {
        vec![PanelMessage::assistant(
            "Pour poursuivre, j'ai besoin des informations suivantes sur l'installation.",
        )]
    });
    let pending = Signal::derive(|| false);
    let interaction = RwSignal::new(Some(interaction_formulaire()));
    let (log, set_log) = signal(String::new());
    view! {
        <div class="flex flex-col h-full">
            <div class="flex-1 min-h-0">
                <AgentPanel
                    messages=messages
                    pending=pending
                    on_send=|_| {}
                    interaction=Signal::derive(move || interaction.get())
                    on_respond=Callback::new(move |resp: InteractionResponse| {
                        let summary = resp.answers.iter().map(|a| {
                            let suffix = a.unsatisfactory_reason.as_deref()
                                .map(|r| format!(" [⚠ {r}]"))
                                .unwrap_or_default();
                            format!("{}: {}{}", a.question_id, a.value, suffix)
                        }).collect::<Vec<_>>().join(" | ");
                        set_log.set(summary);
                        interaction.set(None);
                    })
                />
            </div>
            {move || (!log.get().is_empty()).then(|| view! {
                <div class="border-t border-stone-200 px-2 py-1 text-xs text-stone-500 bg-stone-50 truncate">
                    "Réponses : " {log.get()}
                </div>
            })}
        </div>
    }
}

// ── Scénario 5 : formulaire de confirmation (sélecteur + texte) ───────────────

#[component]
fn ScenarioConfirmation() -> impl IntoView {
    let messages = Signal::derive(|| {
        vec![PanelMessage::assistant(
            "Avant de générer les considérants, confirmez les éléments ci-dessous. \
             Si une réponse ne convient pas, indiquez-le.",
        )]
    });
    let pending = Signal::derive(|| false);
    let interaction = RwSignal::new(Some(interaction_confirmation()));
    let (log, set_log) = signal(String::new());
    view! {
        <div class="flex flex-col h-full">
            <div class="flex-1 min-h-0">
                <AgentPanel
                    messages=messages
                    pending=pending
                    on_send=|_| {}
                    interaction=Signal::derive(move || interaction.get())
                    on_respond=Callback::new(move |resp: InteractionResponse| {
                        let summary = resp.answers.iter().map(|a| {
                            let suffix = a.unsatisfactory_reason.as_deref()
                                .map(|r| format!(" [⚠ {r}]"))
                                .unwrap_or_default();
                            format!("{}: {}{}", a.question_id, a.value, suffix)
                        }).collect::<Vec<_>>().join(" | ");
                        set_log.set(summary);
                        interaction.set(None);
                    })
                />
            </div>
            {move || (!log.get().is_empty()).then(|| view! {
                <div class="border-t border-stone-200 px-2 py-1 text-xs text-stone-500 bg-stone-50 truncate">
                    "Réponses : " {log.get()}
                </div>
            })}
        </div>
    }
}

// ── Scénario 6 : conversation puis formulaire ─────────────────────────────────

#[component]
fn ScenarioConversationPuisFormulaire() -> impl IntoView {
    let base_messages = messages_conversation();
    let (messages, set_messages) = signal(base_messages);
    let pending = Signal::derive(|| false);
    let interaction = RwSignal::new(Some(interaction_formulaire()));

    view! {
        <AgentPanel
            messages=Signal::derive(move || messages.get())
            pending=pending
            on_send=move |text| {
                set_messages.update(|ms| ms.push(PanelMessage::user(text)));
            }
            interaction=Signal::derive(move || interaction.get())
            on_respond=Callback::new(move |resp: InteractionResponse| {
                let summary = resp.answers.iter().map(|a| {
                    let suffix = a.unsatisfactory_reason.as_deref()
                        .map(|r| format!(" (⚠ {r})"))
                        .unwrap_or_default();
                    format!("{} = {}{}", a.question_id, a.value, suffix)
                }).collect::<Vec<_>>().join(", ");
                set_messages.update(|ms| {
                    ms.push(PanelMessage::assistant(
                        format!("Merci. J'ai reçu : {summary}"),
                    ));
                });
                interaction.set(None);
            })
        />
    }
}
