//! Page de démonstration des états du panneau agent IA.
//! Accessible en développement à la route `/dev/agent`.

use agent::{
    AgentPanel, DocumentRequest, DocumentUpload, InteractionRequest, InteractionResponse,
    PanelEntry, PanelQuestion, PanelReasoning, PanelToolCall, PanelToolCallStatus,
};
use leptos::prelude::*;

// ── Scénarios ────────────────────────────────────────────────────────────────

fn messages_conversation() -> Vec<PanelEntry> {
    vec![
        PanelEntry::user("Complète les visas réglementaires"),
        PanelEntry::assistant(
            "J'ai besoin de connaître la rubrique ICPE principale. \
             Quel est le code rubrique de l'installation ?",
        ),
        PanelEntry::user("2760-1"),
        PanelEntry::assistant(
            "Merci. Je recherche les textes applicables à la rubrique 2760-1 \
             et je vais compléter les visas correspondants.",
        ),
        PanelEntry::user("Ajoute aussi le visa relatif au code de l'environnement"),
    ]
}

/// Illustre le tracé des réflexions et des appels d'outils affiché en
/// direct pendant l'exécution de la boucle agentique (voir
/// `agent::AgentObserver`) : une réflexion achevée, un appel d'outil
/// terminé, un autre encore en cours, avant la réponse finale.
fn messages_trace() -> Vec<PanelEntry> {
    vec![
        PanelEntry::user("Complète le considérant relatif au seuil de classement"),
        PanelEntry::Reasoning(PanelReasoning {
            agent_label: "Expert Considérants".to_string(),
            content: "L'utilisateur vise la rubrique 2760-1. Je dois d'abord lire la \
                      structure actuelle de l'acte pour savoir où insérer le considérant, \
                      puis rechercher le seuil réglementaire applicable."
                .to_string(),
            done: true,
        }),
        PanelEntry::ToolCall(PanelToolCall {
            id: "call_1".to_string(),
            agent_label: "Expert Considérants".to_string(),
            name: "read_structure".to_string(),
            arguments: "{}".to_string(),
            status: PanelToolCallStatus::Done {
                output: "{ \"id\": \"root\", \"kind\": \"Root\", \"children\": [] }".to_string(),
            },
        }),
        PanelEntry::ToolCall(PanelToolCall {
            id: "call_2".to_string(),
            agent_label: "Expert Considérants".to_string(),
            name: "legifrance_search".to_string(),
            arguments: "{ \"query\": \"rubrique 2760-1 seuil\" }".to_string(),
            status: PanelToolCallStatus::Running,
        }),
    ]
}

fn interaction_formulaire() -> InteractionRequest {
    InteractionRequest {
        agent_label: "Superviseur".to_string(),
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
        agent_label: "Expert Considérants".to_string(),
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
                <Scenario titre="Réflexions et appels d'outils">
                    <ScenarioTrace/>
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

            <div class="grid grid-cols-2 gap-4">
                <Scenario titre="Conversation + formulaire (enchaînement)">
                    <ScenarioConversationPuisFormulaire/>
                </Scenario>
                <Scenario titre="Demande de document (upload)">
                    <ScenarioDemandeDocument/>
                </Scenario>
            </div>

            <div class="grid grid-cols-2 gap-4 mt-6">
                <Scenario titre="Erreur et arrêt volontaire">
                    <ScenarioErreurEtArret/>
                </Scenario>
                <Scenario titre="Agent en attente — arrêt/redémarrage">
                    <ScenarioEnAttenteAvecControles/>
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
    let messages = Signal::derive(|| Vec::<PanelEntry>::new());
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
    let messages = Signal::derive(|| vec![PanelEntry::user("Rédige les visas de l'arrêté")]);
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

// ── Scénario 3bis : réflexions et appels d'outils tracés en direct ───────────

#[component]
fn ScenarioTrace() -> impl IntoView {
    let messages = Signal::derive(messages_trace);
    let pending = Signal::derive(|| true);
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
        vec![PanelEntry::assistant(
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
        vec![PanelEntry::assistant(
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
                set_messages.update(|ms| ms.push(PanelEntry::user(text)));
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
                    ms.push(PanelEntry::assistant(
                        format!("Merci. J'ai reçu : {summary}"),
                    ));
                });
                interaction.set(None);
            })
        />
    }
}

// ── Scénario 8 : erreur et arrêt volontaire ───────────────────────────────────

#[component]
fn ScenarioErreurEtArret() -> impl IntoView {
    let messages = Signal::derive(|| {
        vec![
            PanelEntry::user("Rédige les visas réglementaires"),
            PanelEntry::error(
                "Expert Visas",
                "le fournisseur du modèle a renvoyé une erreur 503 : service indisponible",
            ),
            PanelEntry::user("Recommence"),
            PanelEntry::stopped("Tâche interrompue par l'utilisateur."),
        ]
    });
    let pending = Signal::derive(|| false);
    let (log, set_log) = signal(String::new());
    view! {
        <div class="flex flex-col h-full">
            <div class="flex-1 min-h-0">
                <AgentPanel
                    messages=messages
                    pending=pending
                    on_send=|_| {}
                    on_restart=Callback::new(move |()| set_log.set("Redémarrage demandé".to_string()))
                />
            </div>
            {move || (!log.get().is_empty()).then(|| view! {
                <div class="border-t border-stone-200 px-2 py-1 text-xs text-stone-500 bg-stone-50 truncate">
                    {log.get()}
                </div>
            })}
        </div>
    }
}

// ── Scénario 9 : agent en attente, avec bouton d'arrêt ────────────────────────

#[component]
fn ScenarioEnAttenteAvecControles() -> impl IntoView {
    let messages = Signal::derive(|| vec![PanelEntry::user("Rédige les visas de l'arrêté")]);
    let pending = RwSignal::new(true);
    let (log, set_log) = signal(String::new());
    view! {
        <div class="flex flex-col h-full">
            <div class="flex-1 min-h-0">
                <AgentPanel
                    messages=messages
                    pending=Signal::derive(move || pending.get())
                    on_send=|_| {}
                    on_stop=Callback::new(move |()| {
                        pending.set(false);
                        set_log.set("Arrêt demandé".to_string());
                    })
                    on_restart=Callback::new(move |()| set_log.set("Redémarrage demandé".to_string()))
                />
            </div>
            {move || (!log.get().is_empty()).then(|| view! {
                <div class="border-t border-stone-200 px-2 py-1 text-xs text-stone-500 bg-stone-50 truncate">
                    {log.get()}
                </div>
            })}
        </div>
    }
}

// ── Scénario 7 : demande de document (upload) ─────────────────────────────────

#[component]
fn ScenarioDemandeDocument() -> impl IntoView {
    let messages = Signal::derive(|| {
        vec![PanelEntry::assistant(
            "Pour vérifier la conformité, transmettez-moi l'étude d'impact au format PDF.",
        )]
    });
    let pending = Signal::derive(|| false);
    let document_request = RwSignal::new(Some(DocumentRequest {
        agent_label: "Superviseur".to_string(),
        prompt: "Étude d'impact (PDF ou ODT)".to_string(),
        accepted_mime_types: vec!["application/pdf".to_string()],
    }));
    let (log, set_log) = signal(String::new());
    view! {
        <div class="flex flex-col h-full">
            <div class="flex-1 min-h-0">
                <AgentPanel
                    messages=messages
                    pending=pending
                    on_send=|_| {}
                    document_request=Signal::derive(move || document_request.get())
                    on_document_response=Callback::new(move |upload: DocumentUpload| {
                        set_log.set(format!(
                            "{} ({}, {} caractères base64)",
                            upload.file_name,
                            upload.mime_type,
                            upload.content_base64.len()
                        ));
                        document_request.set(None);
                    })
                />
            </div>
            {move || (!log.get().is_empty()).then(|| view! {
                <div class="border-t border-stone-200 px-2 py-1 text-xs text-stone-500 bg-stone-50 truncate">
                    "Reçu : " {log.get()}
                </div>
            })}
        </div>
    }
}
