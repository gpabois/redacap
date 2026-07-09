//! Panneau des documents externes fournis pour un projet d'acte légal,
//! affiché dans l'éditeur, à côté des panneaux « Intentions » et
//! « Métadonnées » (voir `crate::pages::project_metadata`). Ces documents
//! sont rattachés au projet lui-même (voir
//! `shared::model::LegalActDocument`, migration `0020_legal_act_documents`)
//! plutôt qu'à une session de conversation avec l'agent : ils restent donc
//! disponibles d'une session à l'autre, et ce panneau permet à l'inspecteur
//! de les gérer directement (upload/suppression), sans passer par l'outil
//! `request_document` de l'agent — qui alimente le même stockage (voir
//! `agent::tools::document`/`agent::tools::interaction`).

use agent::{DocumentUpload, read_uploaded_file};
use dsfr::{Alert, Button, ButtonVariant, Input, Severity, Table};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use shared::broadcast::{DocumentChangeKind, DocumentsChangedEvent};
use web_sys::wasm_bindgen::JsCast;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectDocumentWire {
    id: String,
    file_name: String,
    mime_type: String,
    /// Taille du contenu en octets (voir [`format_size`]).
    size: i64,
    /// Voir [`shared::model::LegalActDocument::label`].
    label: String,
    /// Horodatage RFC 3339 : le client se charge de sa mise en forme.
    created_at: String,
}

/// Forme sur le fil d'un événement diffusé à une salle d'édition, telle
/// qu'attendue côté client par `crate::protocol::ServerMessage` (voir
/// `app::pages::project_metadata` pour la même construction côté métadonnées,
/// et `shared::broadcast` pour l'explication de cet aller-retour de type).
#[cfg(feature = "ssr")]
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutgoingRoomEvent {
    DocumentsChanged(DocumentsChangedEvent),
}

#[cfg(feature = "ssr")]
fn broadcast_documents_change(room_id: &str, event: DocumentsChangedEvent) {
    let broadcaster = expect_context::<shared::broadcast::SharedRoomBroadcaster>();
    if let Ok(payload) = serde_json::to_string(&OutgoingRoomEvent::DocumentsChanged(event)) {
        broadcaster.broadcast(room_id, payload);
    }
}

/// Liste les documents du projet, dans l'ordre où ils ont été fournis (voir
/// `storage::legal_act_document::list_documents_for_legal_act`).
#[server]
async fn list_project_documents(
    legal_act_id: String,
) -> Result<Vec<ProjectDocumentWire>, ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;

    crate::auth::require_legal_act_edit_access(&pool, &user_id, &legal_act_id).await?;

    let documents = storage::legal_act_document::list_documents_for_legal_act(&pool, &legal_act_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(documents
        .into_iter()
        .map(|document| ProjectDocumentWire {
            id: document.id.to_string(),
            file_name: document.file_name,
            mime_type: document.mime_type,
            size: document.size,
            label: document.label,
            created_at: document.created_at.to_rfc3339(),
        })
        .collect())
}

/// Ajoute un document au projet, fourni directement depuis ce panneau
/// (`content_base64` : contenu brut encodé en base64, comme pour la réponse
/// à `request_document`, voir `server::protocol::DocumentUploadWire`).
/// `label` (voir [`shared::model::LegalActDocument::label`]) retombe sur
/// `file_name` si laissé vide, pour qu'un document reste toujours
/// identifiable même sans libellé saisi.
#[server]
async fn upload_project_document(
    legal_act_id: String,
    file_name: String,
    mime_type: String,
    content_base64: String,
    label: String,
) -> Result<(), ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let room_id = legal_act_id.clone();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;
    let file_name = file_name.trim().to_string();
    if file_name.is_empty() {
        return Err(ServerFnError::new("le nom du fichier ne peut pas être vide"));
    }
    let label = label.trim().to_string();

    crate::auth::require_legal_act_edit_access(&pool, &user_id, &legal_act_id).await?;

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&content_base64)
        .map_err(|error| ServerFnError::new(format!("contenu du fichier invalide (base64) : {error}")))?;

    let document = storage::legal_act_document::store_document(
        &pool,
        &legal_act_id,
        &file_name,
        &mime_type,
        bytes,
        &label,
        &user_id,
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    let actor_ip = crate::auth::current_actor_ip().await;
    storage::audit_log::record_audit_event(
        &pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(user_id),
            actor_ip,
            action: "create".to_string(),
            resource_type: "legal_act_document".to_string(),
            resource_id: Some(legal_act_id),
            details: Some(serde_json::json!({ "file_name": document.file_name })),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    broadcast_documents_change(
        &room_id,
        DocumentsChangedEvent {
            file_name: document.file_name,
            kind: DocumentChangeKind::Uploaded,
            by_agent: false,
            actor_id: Some(user_id.to_string()),
        },
    );

    Ok(())
}

/// Supprime un document du projet.
#[server]
async fn delete_project_document(
    legal_act_id: String,
    document_id: String,
) -> Result<(), ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let room_id = legal_act_id.clone();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;
    let document_id: shared::id::ID = document_id
        .parse()
        .map_err(|_| ServerFnError::new("document invalide"))?;

    crate::auth::require_legal_act_edit_access(&pool, &user_id, &legal_act_id).await?;

    let file_name = storage::legal_act_document::delete_document(&pool, &legal_act_id, &document_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let actor_ip = crate::auth::current_actor_ip().await;
    storage::audit_log::record_audit_event(
        &pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(user_id),
            actor_ip,
            action: "delete".to_string(),
            resource_type: "legal_act_document".to_string(),
            resource_id: Some(legal_act_id),
            details: Some(serde_json::json!({ "file_name": file_name })),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    broadcast_documents_change(
        &room_id,
        DocumentsChangedEvent {
            file_name,
            kind: DocumentChangeKind::Deleted,
            by_agent: false,
            actor_id: Some(user_id.to_string()),
        },
    );

    Ok(())
}

/// Met en forme une taille en octets en libellé lisible (Ko/Mo), sans
/// dépendance externe : les documents manipulés ici (PDF, ODT, DOCX...)
/// restent dans une plage où une seule décimale suffit à distinguer deux
/// fichiers proches.
fn format_size(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    if bytes < 1024.0 {
        format!("{bytes:.0} o")
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1} Ko", bytes / 1024.0)
    } else {
        format!("{:.1} Mo", bytes / (1024.0 * 1024.0))
    }
}

/// Panneau affiché dans l'éditeur : liste des documents déjà fournis pour le
/// projet (nom, taille, date, suppression) et un sélecteur de fichier pour
/// en ajouter un nouveau.
#[component]
pub fn ProjectFilesPanel(
    legal_act_id: String,
    /// Incrémenté par [`crate::ws::RoomHandle`] à chaque
    /// [`crate::protocol::ServerMessage::DocumentsChanged`] reçu (ajout ou
    /// suppression par l'agent ou par un autre utilisateur), pour recharger
    /// la liste sans que cette connexion en soit elle-même à l'origine (voir
    /// [`Self::version`] ci-dessous pour ses propres écritures).
    files_version: RwSignal<u32>,
    /// Dernier changement reçu (voir `files_version`), affiché comme avis
    /// ponctuel de ce qui vient de changer et par qui.
    files_last_change: RwSignal<Option<DocumentsChangedEvent>>,
    /// Utilisateur courant, pour ne pas afficher ses propres écritures comme
    /// venant « d'un autre utilisateur » lorsque la diffusion de sa propre
    /// modification revient par le websocket.
    current_user_id: Option<String>,
) -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);

    let id_for_resource = legal_act_id.clone();
    let documents = Resource::new(
        move || (version.get(), files_version.get()),
        move |_| list_project_documents(id_for_resource.clone()),
    );

    let (error, set_error) = signal(Option::<String>::None);
    let (pending_upload, set_pending_upload) = signal(false);
    let (label, set_label) = signal(String::new());

    let id_for_upload = legal_act_id.clone();
    let upload_action = Action::new(move |upload: &DocumentUpload| {
        upload_project_document(
            id_for_upload.clone(),
            upload.file_name.clone(),
            upload.mime_type.clone(),
            upload.content_base64.clone(),
            label.get_untracked(),
        )
    });
    Effect::new(move |_| {
        if let Some(result) = upload_action.value().get() {
            set_pending_upload.set(false);
            match result {
                Ok(()) => {
                    set_error.set(None);
                    set_label.set(String::new());
                    bump();
                }
                Err(error) => set_error.set(Some(error.to_string())),
            }
        }
    });

    let id_for_delete = legal_act_id.clone();
    let delete_action = Action::new(move |document_id: &String| {
        delete_project_document(id_for_delete.clone(), document_id.clone())
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

    let on_file_selected = Callback::new(move |upload: DocumentUpload| {
        set_pending_upload.set(true);
        upload_action.dispatch(upload);
    });

    // Avis ponctuel du dernier changement distant (agent ou autre
    // utilisateur) : masqué pour la propre écriture de cette connexion, qui
    // lui revient par le websocket au même titre qu'aux autres pairs.
    let remote_notice = move || {
        files_last_change.get().and_then(|event| {
            let is_self = !event.by_agent
                && current_user_id.as_deref().is_some_and(|id| Some(id) == event.actor_id.as_deref());
            if is_self {
                return None;
            }
            let action = match event.kind {
                DocumentChangeKind::Uploaded => "ajouté",
                DocumentChangeKind::Deleted => "supprimé",
            };
            let actor = if event.by_agent {
                "par l'agent IA"
            } else {
                "par un autre utilisateur"
            };
            Some(format!("Document « {} » {action} {actor}.", event.file_name))
        })
    };

    view! {
        <div class="px-4 py-3 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 flex flex-col gap-3">
            <span class="text-sm font-bold text-gray-700 dark:text-gray-300">"Fichiers :"</span>
            {move || remote_notice().map(|message| view! {
                <Alert severity=Severity::Info small=true>{message}</Alert>
            })}
            {move || error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <Suspense fallback=|| view! { <span class="text-sm text-gray-500 dark:text-gray-400">"Chargement…"</span> }>
                {move || Suspend::new(async move {
                    match documents.await {
                        Err(_) => view! { <span class="text-sm text-gray-500 dark:text-gray-400">"Fichiers indisponibles."</span> }.into_any(),
                        Ok(items) => {
                            if items.is_empty() {
                                view! {
                                    <span class="text-sm italic text-gray-500 dark:text-gray-400">
                                        "Aucun fichier fourni pour ce projet."
                                    </span>
                                }.into_any()
                            } else {
                                view! {
                                    <Table headers=vec!["Nom", "Libellé", "Taille", ""]>
                                        {items.into_iter().map(|document| {
                                            let id_for_delete = document.id.clone();
                                            let label_display = if document.label.trim().is_empty() {
                                                "—".to_string()
                                            } else {
                                                document.label.clone()
                                            };
                                            view! {
                                                <tr>
                                                    <td class="px-3 py-2 break-all" title=document.mime_type.clone()>{document.file_name.clone()}</td>
                                                    <td class="px-3 py-2 break-all text-gray-600 dark:text-gray-400">{label_display}</td>
                                                    <td class="px-3 py-2 whitespace-nowrap">{format_size(document.size)}</td>
                                                    <td class="px-3 py-2 whitespace-nowrap text-right">
                                                        <Button
                                                            variant=ButtonVariant::TertiaryNoOutline
                                                            disabled=delete_action.pending().get()
                                                            on_click=move |_| { delete_action.dispatch(id_for_delete.clone()); }
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
            <Input
                label="Libellé (optionnel, ex. « rapport d'inspection ICPE »)"
                value=label
                on_input=move |value| set_label.set(value)
            />
            <input
                type="file"
                disabled=move || pending_upload.get()
                class="text-sm text-gray-900 dark:text-gray-100"
                on:change=move |ev| {
                    if let Some(input) = ev
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                    {
                        read_uploaded_file(input, on_file_selected);
                    }
                }
            />
            {move || pending_upload.get().then(|| view! {
                <span class="text-sm text-gray-500 dark:text-gray-400">"Envoi en cours…"</span>
            })}
        </div>
    }
}
