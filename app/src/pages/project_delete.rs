//! Suppression d'un projet d'acte légal, depuis le tableau de bord ou depuis
//! l'éditeur — voir `crate::pages::dashboard` et `crate::app::PageEditorProjet`.
//! N'est proposée que si l'utilisateur dispose du droit d'édition sur le
//! projet (voir `crate::auth::require_legal_act_edit_access`), vérifié à la
//! fois côté UI (pour masquer l'action) et côté serveur (défense en
//! profondeur). La suppression est irréversible : elle est gardée par une
//! confirmation explicite, annulable tant qu'elle n'a pas été validée.

use dsfr::{Alert, Button, ButtonVariant, Severity};
use leptos::prelude::*;

/// Supprime définitivement un projet d'acte légal (voir
/// `storage::legal_act::delete_legal_act` pour l'étendue de ce qui est
/// purgé) et journalise l'action dans le journal d'audit.
#[server]
async fn delete_project(legal_act_id: String) -> Result<(), ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;

    crate::auth::require_legal_act_edit_access(&pool, &user_id, &legal_act_id).await?;

    storage::legal_act::delete_legal_act(&pool, &legal_act_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let actor_ip = crate::auth::current_actor_ip().await;
    storage::audit_log::record_audit_event(
        &pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(user_id),
            actor_ip,
            action: "delete".to_string(),
            resource_type: "legal_act".to_string(),
            resource_id: Some(legal_act_id),
            details: None,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(())
}

/// Indique si l'utilisateur courant a le droit de supprimer (= d'éditer, voir
/// `crate::auth::require_legal_act_edit_access`) le projet `legal_act_id` —
/// utilisé pour n'afficher l'action de suppression que si elle est
/// effectivement permise.
#[server]
async fn can_delete_project(legal_act_id: String) -> Result<bool, ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;
    Ok(
        crate::auth::require_legal_act_edit_access(&pool, &user_id, &legal_act_id)
            .await
            .is_ok(),
    )
}

/// Bouton de suppression d'un projet, avec confirmation explicite et
/// possibilité d'annuler avant exécution : un premier clic affiche un
/// message de confirmation avec deux actions distinctes (« Annuler » /
/// « Confirmer la suppression ») plutôt que d'exécuter la suppression
/// immédiatement ou de la déclencher sur un simple second clic.
#[component]
pub fn DeleteProjectButton(
    legal_act_id: String,
    /// Appelé après suppression réussie (retirer la ligne du tableau de
    /// bord, ou quitter l'éditeur).
    on_deleted: Callback<()>,
) -> impl IntoView {
    let (confirming, set_confirming) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let delete_action = Action::new(move |_: &()| delete_project(legal_act_id.clone()));
    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => {
                    set_confirming.set(false);
                    on_deleted.run(());
                }
                Err(error) => set_error.set(Some(error.to_string())),
            }
        }
    });

    view! {
        {move || error.get().map(|message| view! {
            <Alert severity=Severity::Error small=true>{message}</Alert>
        })}
        {move || if confirming.get() {
            view! {
                <span class="inline-flex items-center gap-2">
                    <span class="text-sm text-gray-700 dark:text-gray-300 whitespace-nowrap">
                        "Supprimer définitivement ce projet ?"
                    </span>
                    <Button
                        variant=ButtonVariant::Tertiary
                        disabled=delete_action.pending().get()
                        on_click=move |_| set_confirming.set(false)
                    >
                        "Annuler"
                    </Button>
                    <Button
                        variant=ButtonVariant::Secondary
                        disabled=delete_action.pending().get()
                        on_click=move |_| { delete_action.dispatch(()); }
                    >
                        {move || if delete_action.pending().get() { "Suppression…" } else { "Confirmer la suppression" }}
                    </Button>
                </span>
            }.into_any()
        } else {
            view! {
                <Button
                    variant=ButtonVariant::Tertiary
                    on_click=move |_| set_confirming.set(true)
                >
                    "Supprimer"
                </Button>
            }.into_any()
        }}
    }
}

/// Section « Zone de danger » affichée dans l'onglet Paramètres de
/// l'éditeur : ne montre l'action de suppression que si l'utilisateur
/// courant a le droit d'éditer ce projet (voir [`can_delete_project`]).
#[component]
pub fn ProjectDangerZone(
    legal_act_id: String,
    /// Appelé après suppression réussie, pour que la page hôte quitte
    /// l'éditeur (voir `crate::app::PageEditorProjet`).
    on_deleted: Callback<()>,
) -> impl IntoView {
    let id_for_check = legal_act_id.clone();
    let can_delete = Resource::new(|| (), move |_| can_delete_project(id_for_check.clone()));

    view! {
        <div class="px-4 py-3 border-t border-gray-200 dark:border-gray-800 flex flex-col gap-2">
            <Suspense fallback=|| ()>
                {move || {
                    let legal_act_id = legal_act_id.clone();
                    Suspend::new(async move {
                        matches!(can_delete.await, Ok(true)).then(move || view! {
                            <span class="text-xs font-bold uppercase tracking-wide text-gray-500 dark:text-gray-400">
                                "Zone de danger"
                            </span>
                            <DeleteProjectButton legal_act_id=legal_act_id.clone() on_deleted=on_deleted/>
                        })
                    })
                }}
            </Suspense>
        </div>
    }
}
