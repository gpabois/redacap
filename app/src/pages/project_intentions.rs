//! Panneau des intentions d'un projet d'acte légal (ex. « mise en demeure »,
//! « sanction administrative »), affiché dans l'éditeur — voir `Claude.md`
//! § « Ajoute aux projets... ». Seules les intentions rattachées au domaine
//! du projet (fixé à sa création, voir `crate::pages::editor_new`) peuvent y
//! être ajoutées ; l'ajout/retrait est immédiat, sans passer par le corps
//! CRDT de l'acte (les intentions sont une relation métier, pas un nœud du
//! corps).

use dsfr::{Alert, Button, ButtonVariant, Select, SelectOption, Severity, Tag, TagGroup};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IntentionOption {
    id: String,
    name: String,
    selected: bool,
}

/// Vérifie que l'utilisateur courant peut éditer le projet `legal_act_id`
/// (auteur, ou droit direct/hérité de ses groupes — voir
/// `crate::auth::accessible_legal_act_ids`), et renvoie le projet.
#[cfg(feature = "ssr")]
async fn require_legal_act_access(
    pool: &storage::Pool,
    user_id: &shared::id::ID,
    legal_act_id: &shared::id::ID,
) -> Result<shared::model::LegalAct, ServerFnError> {
    let legal_act = storage::legal_act::get_legal_act(pool, legal_act_id)
        .await
        .map_err(|_| ServerFnError::new("projet introuvable"))?;
    if legal_act.created_by != *user_id {
        let accessible_ids = crate::auth::accessible_legal_act_ids(pool, user_id).await?;
        if !accessible_ids.contains(&legal_act.id) {
            return Err(ServerFnError::new(
                "vous n'avez pas le droit d'éditer ce projet",
            ));
        }
    }
    Ok(legal_act)
}

/// Liste les intentions du domaine du projet, avec leur état d'association
/// actuel (voir `storage::intention::list_intentions_by_domain`/
/// `list_intentions_for_legal_act`).
#[server]
async fn list_project_intentions(
    legal_act_id: String,
) -> Result<Vec<IntentionOption>, ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;

    let legal_act = require_legal_act_access(&pool, &user_id, &legal_act_id).await?;

    let domain_intentions =
        storage::intention::list_intentions_by_domain(&pool, &legal_act.domain_id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
    let attached = storage::intention::list_intentions_for_legal_act(&pool, &legal_act_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let attached_ids: std::collections::HashSet<shared::id::ID> =
        attached.into_iter().map(|intention| intention.id).collect();

    Ok(domain_intentions
        .into_iter()
        .map(|intention| IntentionOption {
            selected: attached_ids.contains(&intention.id),
            id: intention.id.to_string(),
            name: intention.name,
        })
        .collect())
}

/// Associe une intention au projet. Refuse si l'intention n'appartient pas au
/// domaine du projet (seules les intentions du domaine peuvent être
/// sélectionnées, voir `Claude.md`).
#[server]
async fn add_project_intention(
    legal_act_id: String,
    intention_id: String,
) -> Result<(), ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;
    let intention_id: shared::id::ID = intention_id
        .parse()
        .map_err(|_| ServerFnError::new("intention invalide"))?;

    let legal_act = require_legal_act_access(&pool, &user_id, &legal_act_id).await?;

    let intention = storage::intention::get_intention(&pool, &intention_id)
        .await
        .map_err(|_| ServerFnError::new("intention introuvable"))?;
    if intention.domain_id != legal_act.domain_id {
        return Err(ServerFnError::new(
            "cette intention n'appartient pas au domaine du projet",
        ));
    }

    storage::intention::add_intention_to_legal_act(&pool, &legal_act_id, &intention_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let actor_ip = crate::auth::current_actor_ip().await;
    storage::audit_log::record_audit_event(
        &pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(user_id),
            actor_ip,
            action: "add".to_string(),
            resource_type: "legal_act_intention".to_string(),
            resource_id: Some(intention_id),
            details: None,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(())
}

/// Retire une intention du projet.
#[server]
async fn remove_project_intention(
    legal_act_id: String,
    intention_id: String,
) -> Result<(), ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("projet invalide"))?;
    let intention_id: shared::id::ID = intention_id
        .parse()
        .map_err(|_| ServerFnError::new("intention invalide"))?;

    require_legal_act_access(&pool, &user_id, &legal_act_id).await?;

    storage::intention::remove_intention_from_legal_act(&pool, &legal_act_id, &intention_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let actor_ip = crate::auth::current_actor_ip().await;
    storage::audit_log::record_audit_event(
        &pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(user_id),
            actor_ip,
            action: "remove".to_string(),
            resource_type: "legal_act_intention".to_string(),
            resource_id: Some(intention_id),
            details: None,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(())
}

/// Panneau affiché dans l'éditeur : intentions déjà associées au projet
/// (retirables), et sélecteur pour en ajouter parmi celles du domaine du
/// projet pas encore associées.
#[component]
pub fn ProjectIntentionsPanel(legal_act_id: String) -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);

    let id_for_resource = legal_act_id.clone();
    let intentions = Resource::new(
        move || version.get(),
        move |_| list_project_intentions(id_for_resource.clone()),
    );

    let (add_selection, set_add_selection) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);

    let id_for_add = legal_act_id.clone();
    let add_action = Action::new(move |intention_id: &String| {
        add_project_intention(id_for_add.clone(), intention_id.clone())
    });
    Effect::new(move |_| {
        if let Some(result) = add_action.value().get() {
            match result {
                Ok(()) => {
                    set_add_selection.set(String::new());
                    set_error.set(None);
                    bump();
                }
                Err(error) => set_error.set(Some(error.to_string())),
            }
        }
    });

    let id_for_remove = legal_act_id.clone();
    let remove_action = Action::new(move |intention_id: &String| {
        remove_project_intention(id_for_remove.clone(), intention_id.clone())
    });
    Effect::new(move |_| {
        if let Some(result) = remove_action.value().get() {
            match result {
                Ok(()) => {
                    set_error.set(None);
                    bump();
                }
                Err(error) => set_error.set(Some(error.to_string())),
            }
        }
    });

    view! {
        <div class="px-4 py-2 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 flex flex-wrap items-center gap-3">
            <span class="text-sm font-bold text-gray-700 dark:text-gray-300 whitespace-nowrap">"Intentions :"</span>
            {move || error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <Suspense fallback=|| view! { <span class="text-sm text-gray-500 dark:text-gray-400">"Chargement…"</span> }>
                {move || Suspend::new(async move {
                    match intentions.await {
                        Err(_) => view! { <span class="text-sm text-gray-500 dark:text-gray-400">"Intentions indisponibles."</span> }.into_any(),
                        Ok(options) => {
                            let attached: Vec<_> = options.iter().filter(|option| option.selected).cloned().collect();
                            let available: Vec<_> = options.into_iter().filter(|option| !option.selected).collect();
                            let mut select_options = vec![SelectOption::new("", "— Ajouter une intention —")];
                            select_options.extend(available.iter().map(|option| SelectOption::new(option.id.clone(), option.name.clone())));
                            let has_available = !available.is_empty();
                            view! {
                                <div class="flex flex-wrap items-center gap-3">
                                    <TagGroup>
                                        {attached.into_iter().map(|intention| {
                                            let intention_id = intention.id.clone();
                                            view! {
                                                <li>
                                                    <Tag
                                                        on_click=|_| {}
                                                        on_dismiss=Callback::new(move |_| {
                                                            remove_action.dispatch(intention_id.clone());
                                                        })
                                                    >
                                                        {intention.name}
                                                    </Tag>
                                                </li>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </TagGroup>
                                    {has_available.then(|| view! {
                                        <div class="flex items-center gap-2">
                                            <Select
                                                label=""
                                                options=select_options.clone()
                                                value=add_selection
                                                on_change=move |value| set_add_selection.set(value)
                                            />
                                            <Button
                                                variant=ButtonVariant::Secondary
                                                disabled=add_selection.get().is_empty() || add_action.pending().get()
                                                on_click=move |_| {
                                                    let value = add_selection.get();
                                                    if !value.is_empty() {
                                                        add_action.dispatch(value);
                                                    }
                                                }
                                            >
                                                "Ajouter"
                                            </Button>
                                        </div>
                                    })}
                                </div>
                            }.into_any()
                        }
                    }
                })}
            </Suspense>
        </div>
    }
}
