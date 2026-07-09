//! Panneau des métadonnées contextuelles d'un projet d'acte légal
//! (installation, rubriques ICPE, émissaires...), affiché dans l'éditeur, à
//! côté du panneau des intentions (voir `crate::pages::project_intentions`).
//! Ces métadonnées sont des paires clé/valeur JSON libre, également
//! accessibles à l'agent IA via les outils `read_metadata`/`write_metadata`/
//! `search_metadata` (voir `agent::tools::metadata`) : ce panneau ne fait que
//! les administrer manuellement, sans passer par le corps CRDT de l'acte.

use dsfr::{Alert, Button, ButtonVariant, Input, Severity, Table};
use leptos::prelude::*;
use pulldown_cmark::{Event, Options, Parser};
use serde::{Deserialize, Serialize};
use shared::broadcast::{MetadataChangeKind, MetadataChangedEvent};

/// Clé conventionnelle de la todo-list tenue par le Superviseur au fil de ses
/// délégations `delegate_to_expert` (voir `SUPERVISOR_SYSTEM_PROMPT`,
/// `server::editor::ws`, et `experts.md`) : réservée au Superviseur, aucun
/// profil d'expert ne doit la lire ni l'écrire. Ce panneau ne fait qu'en
/// afficher l'état sous forme de liste à cocher plutôt que le JSON brut.
const TODO_SUPERVISEUR_KEY: &str = "todo_superviseur";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetadataEntryWire {
    key: String,
    /// Représentation textuelle de la valeur : la chaîne elle-même si la
    /// métadonnée est une valeur JSON `String` (cas courant, y compris pour
    /// celles créées depuis ce panneau), sinon sa sérialisation JSON brute
    /// (métadonnée structurée écrite par l'agent, ex. un objet ou un tableau).
    value: String,
    /// `true` si la valeur d'origine n'est pas une simple chaîne JSON (objet,
    /// tableau, nombre, booléen...), pour distinguer à l'affichage une
    /// métadonnée structurée (rendue en JSON mis en forme, ou en liste à
    /// cocher pour [`TODO_SUPERVISEUR_KEY`]) d'un texte libre (rendu en
    /// Markdown, voir [`render_markdown`]).
    is_structured: bool,
}

/// Sous-tâche de la todo-list du Superviseur (voir [`TODO_SUPERVISEUR_KEY`]) :
/// `statut` vaut `"a_faire"` ou `"fait"`.
#[derive(Debug, Clone, Deserialize)]
struct TodoSuperviseurItem {
    tache: String,
    statut: String,
}

/// Convertit un texte Markdown (note manuelle ou métadonnée texte libre de
/// l'agent) en HTML affichable.
///
/// Les balises HTML brutes présentes dans la source sont supprimées : cette
/// valeur peut refléter du contenu utilisateur, donc on ne peut pas lui faire
/// confiance pour injecter du HTML tel quel (XSS) — voir la même précaution
/// dans `agent::panel::render_markdown`.
fn render_markdown(source: &str) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options)
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);
    html_output
}

/// Affiche la valeur d'une métadonnée dans la cellule « Valeur » du tableau :
/// liste à cocher (lecture seule) pour [`TODO_SUPERVISEUR_KEY`] lorsque la
/// valeur a la forme attendue, JSON mis en forme pour toute autre métadonnée
/// structurée, Markdown rendu pour un texte libre.
fn render_metadata_value(key: &str, value: &str, is_structured: bool) -> AnyView {
    if key == TODO_SUPERVISEUR_KEY {
        if let Ok(items) = serde_json::from_str::<Vec<TodoSuperviseurItem>>(value) {
            return view! {
                <ul class="flex flex-col gap-1">
                    {items.into_iter().map(|item| {
                        let done = item.statut == "fait";
                        view! {
                            <li class="flex items-center gap-2">
                                <input
                                    type="checkbox"
                                    class="size-4 accent-blue-france"
                                    prop:checked=done
                                    disabled=true
                                />
                                <span class=if done { "line-through text-gray-500 dark:text-gray-400" } else { "" }>
                                    {item.tache}
                                </span>
                            </li>
                        }
                    }).collect::<Vec<_>>()}
                </ul>
            }.into_any();
        }
    }

    if is_structured {
        let pretty = serde_json::from_str::<serde_json::Value>(value)
            .ok()
            .and_then(|parsed| serde_json::to_string_pretty(&parsed).ok())
            .unwrap_or_else(|| value.to_string());
        view! { <pre class="text-xs font-mono whitespace-pre-wrap">{pretty}</pre> }.into_any()
    } else {
        view! { <div class="markdown-content text-sm" inner_html=render_markdown(value)></div> }.into_any()
    }
}

/// Forme sur le fil d'un événement diffusé à une salle d'édition, telle
/// qu'attendue côté client par `crate::protocol::ServerMessage` (voir
/// `server::editor::protocol::ServerMessage`, dont ce module ne peut pas
/// dépendre directement — voir `shared::broadcast`). Un seul variant pour
/// l'instant : ce panneau est la seule `ServerFunction` à devoir notifier une
/// salle en dehors de la boucle websocket elle-même. `#[cfg(ssr)]` comme
/// [`broadcast_metadata_change`] : sans effet côté client, où le corps des
/// `#[server]` ci-dessous n'est de toute façon pas compilé.
#[cfg(feature = "ssr")]
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutgoingRoomEvent {
    MetadataChanged(MetadataChangedEvent),
}

/// Notifie, si le contexte de diffusion est disponible (voir
/// `shared::broadcast::SharedRoomBroadcaster`, injecté par `server::run`),
/// tous les pairs connectés à la salle d'édition `room_id` du changement
/// `event` : sans effet si la sérialisation échoue (ne devrait pas arriver)
/// ou si aucune connexion n'est actuellement ouverte sur cette salle — dans
/// les deux cas, la métadonnée elle-même a déjà été écrite/supprimée avec
/// succès (voir les appelants), cette notification n'est qu'un confort
/// d'affichage temps réel.
#[cfg(feature = "ssr")]
fn broadcast_metadata_change(room_id: &str, event: MetadataChangedEvent) {
    let broadcaster = expect_context::<shared::broadcast::SharedRoomBroadcaster>();
    if let Ok(payload) = serde_json::to_string(&OutgoingRoomEvent::MetadataChanged(event)) {
        broadcaster.broadcast(room_id, payload);
    }
}

/// Liste les métadonnées du projet, triées par clé (voir
/// `storage::legal_act_metadata::list_metadata`).
#[server]
async fn list_project_metadata(legal_act_id: String) -> Result<Vec<MetadataEntryWire>, ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;

    crate::auth::require_legal_act_edit_access(&pool, &user_id, &legal_act_id).await?;

    let entries = storage::legal_act_metadata::list_metadata(&pool, &legal_act_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(entries
        .into_iter()
        .map(|entry| MetadataEntryWire {
            key: entry.key,
            is_structured: !entry.value.is_string(),
            value: entry
                .value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| entry.value.to_string()),
        })
        .collect())
}

/// Crée ou remplace une métadonnée du projet, en valeur JSON `String`.
#[server]
async fn set_project_metadata(
    legal_act_id: String,
    key: String,
    value: String,
) -> Result<(), ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let room_id = legal_act_id.clone();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(ServerFnError::new("la clé ne peut pas être vide"));
    }

    crate::auth::require_legal_act_edit_access(&pool, &user_id, &legal_act_id).await?;

    let entry = storage::legal_act_metadata::upsert_metadata(
        &pool,
        &legal_act_id,
        &key,
        serde_json::Value::String(value),
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    let actor_ip = crate::auth::current_actor_ip().await;
    storage::audit_log::record_audit_event(
        &pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(user_id),
            actor_ip,
            action: "write".to_string(),
            resource_type: "legal_act_metadata".to_string(),
            resource_id: Some(legal_act_id),
            details: Some(serde_json::json!({ "key": key })),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    // Voir `server::editor::ports::WsMetadata::write` pour la même
    // distinction création/mise à jour à partir de `created_at`/`updated_at`.
    let kind = if entry.created_at == entry.updated_at {
        MetadataChangeKind::Created
    } else {
        MetadataChangeKind::Updated
    };
    broadcast_metadata_change(
        &room_id,
        MetadataChangedEvent {
            key,
            kind,
            by_agent: false,
            actor_id: Some(user_id.to_string()),
        },
    );

    Ok(())
}

/// Supprime une métadonnée du projet.
#[server]
async fn delete_project_metadata(legal_act_id: String, key: String) -> Result<(), ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let room_id = legal_act_id.clone();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;

    crate::auth::require_legal_act_edit_access(&pool, &user_id, &legal_act_id).await?;

    storage::legal_act_metadata::delete_metadata(&pool, &legal_act_id, &key)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let actor_ip = crate::auth::current_actor_ip().await;
    storage::audit_log::record_audit_event(
        &pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(user_id),
            actor_ip,
            action: "delete".to_string(),
            resource_type: "legal_act_metadata".to_string(),
            resource_id: Some(legal_act_id),
            details: Some(serde_json::json!({ "key": key })),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    broadcast_metadata_change(
        &room_id,
        MetadataChangedEvent {
            key,
            kind: MetadataChangeKind::Deleted,
            by_agent: false,
            actor_id: Some(user_id.to_string()),
        },
    );

    Ok(())
}

/// Panneau affiché dans l'éditeur : tableau des métadonnées déjà
/// renseignées (modifiables/supprimables), et un unique formulaire
/// clé/valeur pour en enregistrer une (« Modifier » sur une ligne ne fait
/// que pré-remplir ce formulaire — la validation fait toujours un upsert :
/// même clé = mise à jour de sa valeur, clé différente = nouvelle entrée).
#[component]
pub fn ProjectMetadataPanel(
    legal_act_id: String,
    /// Incrémenté par [`crate::ws::RoomHandle`] à chaque
    /// [`crate::protocol::ServerMessage::MetadataChanged`] reçu (écriture ou
    /// suppression par l'agent ou par un autre utilisateur), pour recharger
    /// la liste sans que cette connexion en soit elle-même à l'origine (voir
    /// [`Self::version`] ci-dessous pour ses propres écritures).
    metadata_version: RwSignal<u32>,
    /// Dernier changement reçu (voir `metadata_version`), affiché comme avis
    /// ponctuel de ce qui vient de changer et par qui.
    metadata_last_change: RwSignal<Option<MetadataChangedEvent>>,
    /// Utilisateur courant, pour ne pas afficher ses propres écritures comme
    /// venant « d'un autre utilisateur » lorsque la diffusion de sa propre
    /// modification revient par le websocket (voir `metadata_last_change`).
    current_user_id: Option<String>,
) -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);

    let id_for_resource = legal_act_id.clone();
    let entries = Resource::new(
        move || (version.get(), metadata_version.get()),
        move |_| list_project_metadata(id_for_resource.clone()),
    );

    let (draft_key, set_draft_key) = signal(String::new());
    let (draft_value, set_draft_value) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);

    let id_for_set = legal_act_id.clone();
    let set_action = Action::new(move |(key, value): &(String, String)| {
        set_project_metadata(id_for_set.clone(), key.clone(), value.clone())
    });
    Effect::new(move |_| {
        if let Some(result) = set_action.value().get() {
            match result {
                Ok(()) => {
                    set_draft_key.set(String::new());
                    set_draft_value.set(String::new());
                    set_error.set(None);
                    bump();
                }
                Err(error) => set_error.set(Some(error.to_string())),
            }
        }
    });

    let id_for_delete = legal_act_id.clone();
    let delete_action = Action::new(move |key: &String| {
        delete_project_metadata(id_for_delete.clone(), key.clone())
    });
    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => {
                    set_error.set(None);
                    bump();
                }
                Err(error) => set_error.set(Some(error.to_string())),
            }
        }
    });

    // Pré-remplit le formulaire avec une entrée existante : soumettre sans
    // changer la clé écrase sa valeur (modification), la changer crée une
    // nouvelle entrée sans toucher à l'ancienne.
    let start_edit = move |key: String, value: String| {
        set_draft_key.set(key);
        set_draft_value.set(value);
    };
    let submit = move |_| {
        let key = draft_key.get().trim().to_string();
        if key.is_empty() {
            return;
        }
        set_action.dispatch((key, draft_value.get()));
    };

    // Avis ponctuel du dernier changement distant (agent ou autre
    // utilisateur) : masqué pour la propre écriture de cette connexion, qui
    // lui revient par le websocket au même titre qu'aux autres pairs (voir
    // `crate::ws::RoomHandle::metadata_last_change`) mais s'affiche déjà via
    // le formulaire qui vient d'être vidé.
    let remote_notice = move || {
        metadata_last_change.get().and_then(|event| {
            let is_self = !event.by_agent
                && current_user_id.as_deref().is_some_and(|id| Some(id) == event.actor_id.as_deref());
            if is_self {
                return None;
            }
            let action = match event.kind {
                MetadataChangeKind::Created => "créée",
                MetadataChangeKind::Updated => "modifiée",
                MetadataChangeKind::Deleted => "supprimée",
            };
            let actor = if event.by_agent {
                "par l'agent IA"
            } else {
                "par un autre utilisateur"
            };
            Some(format!("Métadonnée « {} » {action} {actor}.", event.key))
        })
    };

    view! {
        <div class="px-4 py-3 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 flex flex-col gap-3">
            <span class="text-sm font-bold text-gray-700 dark:text-gray-300">"Métadonnées :"</span>
            {move || remote_notice().map(|message| view! {
                <Alert severity=Severity::Info small=true>{message}</Alert>
            })}
            {move || error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <Suspense fallback=|| view! { <span class="text-sm text-gray-500 dark:text-gray-400">"Chargement…"</span> }>
                {move || Suspend::new(async move {
                    match entries.await {
                        Err(_) => view! { <span class="text-sm text-gray-500 dark:text-gray-400">"Métadonnées indisponibles."</span> }.into_any(),
                        Ok(items) => {
                            if items.is_empty() {
                                view! {
                                    <span class="text-sm italic text-gray-500 dark:text-gray-400">
                                        "Aucune métadonnée renseignée."
                                    </span>
                                }.into_any()
                            } else {
                                view! {
                                    <Table headers=vec!["Clé", "Valeur", ""]>
                                        {items.into_iter().map(|entry| {
                                            let key_for_edit = entry.key.clone();
                                            let value_for_edit = entry.value.clone();
                                            let key_for_delete = entry.key.clone();
                                            let rendered_value = render_metadata_value(&entry.key, &entry.value, entry.is_structured);
                                            view! {
                                                <tr>
                                                    <td class="px-3 py-2 font-mono">{entry.key.clone()}</td>
                                                    <td class="px-3 py-2 break-words">{rendered_value}</td>
                                                    <td class="px-3 py-2 whitespace-nowrap text-right">
                                                        <Button
                                                            variant=ButtonVariant::TertiaryNoOutline
                                                            on_click=move |_| start_edit(key_for_edit.clone(), value_for_edit.clone())
                                                        >
                                                            "Modifier"
                                                        </Button>
                                                        <Button
                                                            variant=ButtonVariant::TertiaryNoOutline
                                                            disabled=delete_action.pending().get()
                                                            on_click=move |_| { delete_action.dispatch(key_for_delete.clone()); }
                                                        >
                                                            "Supprimer"
                                                        </Button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </Table>
                                }.into_any()
                            }
                        }
                    }
                })}
            </Suspense>
            <div class="flex flex-wrap items-end gap-2">
                <Input
                    label="Clé"
                    value=draft_key
                    on_input=move |value| set_draft_key.set(value)
                />
                <Input
                    label="Valeur"
                    value=draft_value
                    on_input=move |value| set_draft_value.set(value)
                />
                <Button
                    variant=ButtonVariant::Secondary
                    disabled=draft_key.get().trim().is_empty() || set_action.pending().get()
                    on_click=submit
                >
                    "Enregistrer"
                </Button>
            </div>
        </div>
    }
}
