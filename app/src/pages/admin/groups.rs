//! Page `/admin/groups` : gestion de la hiérarchie de groupes (une entité
//! correspond à un groupe possédant des sous-groupes) — voir `Claude.md`
//! § Pages de l'application.

use std::collections::HashMap;

use dsfr::{Alert, Button, ButtonVariant, Input, Select, SelectOption, Severity};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    AdminAccessDenied, AdminHeader, AdminNav, AdminSection, ConfirmButton, PermissionsPanel,
    admin_context,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupRow {
    id: String,
    parent_id: Option<String>,
    name: String,
}

#[server]
async fn list_all_groups_admin() -> Result<Vec<GroupRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let groups = storage::group::list_all_groups(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(groups
        .into_iter()
        .map(|group| GroupRow {
            id: group.id.to_string(),
            parent_id: group.parent_group_id.map(|id| id.to_string()),
            name: group.name,
        })
        .collect())
}

#[server]
async fn create_group_admin(name: String, parent_id: Option<String>) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("le nom du groupe est obligatoire"));
    }
    let parent_group_id = match parent_id.filter(|id| !id.is_empty()) {
        Some(id) => Some(
            id.parse::<shared::id::ID>()
                .map_err(|_| ServerFnError::new("groupe parent invalide"))?,
        ),
        None => None,
    };

    let group = storage::group::create_group(
        &pool,
        shared::model::CreateGroup {
            name,
            parent_group_id,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "create", "group", Some(group.id)).await
}

#[server]
async fn rename_group_admin(group_id: String, name: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("le nom du groupe est obligatoire"));
    }
    let group_id: shared::id::ID = group_id
        .parse()
        .map_err(|_| ServerFnError::new("groupe invalide"))?;

    storage::group::update_group(
        &pool,
        &group_id,
        shared::model::GroupChangeset {
            name: Some(name),
            ..Default::default()
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "group", Some(group_id)).await
}

#[server]
async fn reparent_group_admin(
    group_id: String,
    parent_id: Option<String>,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let group_id: shared::id::ID = group_id
        .parse()
        .map_err(|_| ServerFnError::new("groupe invalide"))?;
    let parent_group_id = match parent_id.filter(|id| !id.is_empty()) {
        Some(id) => Some(
            id.parse::<shared::id::ID>()
                .map_err(|_| ServerFnError::new("groupe parent invalide"))?,
        ),
        None => None,
    };

    if let Some(parent_group_id) = parent_group_id {
        if parent_group_id == group_id {
            return Err(ServerFnError::new(
                "un groupe ne peut pas être son propre parent",
            ));
        }
        let descendants = storage::group::list_descendant_groups(&pool, &group_id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
        if descendants.iter().any(|group| group.id == parent_group_id) {
            return Err(ServerFnError::new(
                "impossible de déplacer un groupe sous l'un de ses propres descendants",
            ));
        }
    }

    storage::group::update_group(
        &pool,
        &group_id,
        shared::model::GroupChangeset {
            parent_group_id: Some(parent_group_id),
            ..Default::default()
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "group", Some(group_id)).await
}

#[server]
async fn delete_group_admin(group_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let group_id: shared::id::ID = group_id
        .parse()
        .map_err(|_| ServerFnError::new("groupe invalide"))?;

    storage::group::delete_group(&pool, &group_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "delete", "group", Some(group_id)).await
}

/// Aplatit la hiérarchie de groupes en une liste ordonnée en profondeur,
/// chaque entrée portant son niveau d'indentation.
fn flatten_tree(groups: Vec<GroupRow>) -> Vec<(GroupRow, usize)> {
    let mut by_parent: HashMap<Option<String>, Vec<GroupRow>> = HashMap::new();
    for group in groups {
        by_parent
            .entry(group.parent_id.clone())
            .or_default()
            .push(group);
    }
    for children in by_parent.values_mut() {
        children.sort_by(|a, b| a.name.cmp(&b.name));
    }

    fn visit(
        parent: Option<String>,
        depth: usize,
        by_parent: &HashMap<Option<String>, Vec<GroupRow>>,
        result: &mut Vec<(GroupRow, usize)>,
    ) {
        if let Some(children) = by_parent.get(&parent) {
            for child in children {
                result.push((child.clone(), depth));
                visit(Some(child.id.clone()), depth + 1, by_parent, result);
            }
        }
    }

    let mut result = Vec::new();
    visit(None, 0, &by_parent, &mut result);
    result
}

#[component]
pub fn PageAdminGroups() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::Groups/>
                            <div class="max-w-6xl mx-auto p-6">
                                <GroupsPanel is_super_admin=access.is_super_admin/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn GroupsPanel(is_super_admin: bool) -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);

    let groups = Resource::new(move || version.get(), |_| list_all_groups_admin());
    let selected_group = RwSignal::new(Option::<String>::None);

    let (new_name, set_new_name) = signal(String::new());
    let (new_parent, set_new_parent) = signal(String::new());
    let (form_error, set_form_error) = signal(Option::<String>::None);

    let create_action = Action::new(move |input: &(String, String)| {
        let (name, parent_id) = input.clone();
        create_group_admin(name, (!parent_id.is_empty()).then_some(parent_id))
    });
    Effect::new(move |_| {
        if let Some(result) = create_action.value().get() {
            match result {
                Ok(()) => {
                    set_new_name.set(String::new());
                    set_new_parent.set(String::new());
                    set_form_error.set(None);
                    bump();
                }
                Err(error) => set_form_error.set(Some(error.to_string())),
            }
        }
    });

    let rename_action = Action::new(|input: &(String, String)| {
        let (group_id, name) = input.clone();
        rename_group_admin(group_id, name)
    });
    Effect::new(move |_| {
        if let Some(Ok(())) = rename_action.value().get() {
            bump();
        }
    });

    let reparent_action = Action::new(|input: &(String, String)| {
        let (group_id, parent_id) = input.clone();
        reparent_group_admin(group_id, (!parent_id.is_empty()).then_some(parent_id))
    });
    let (reparent_error, set_reparent_error) = signal(Option::<String>::None);
    Effect::new(move |_| {
        if let Some(result) = reparent_action.value().get() {
            match result {
                Ok(()) => {
                    set_reparent_error.set(None);
                    bump();
                }
                Err(error) => set_reparent_error.set(Some(error.to_string())),
            }
        }
    });

    let delete_action = Action::new(|group_id: &String| delete_group_admin(group_id.clone()));
    Effect::new(move |_| {
        if let Some(Ok(())) = delete_action.value().get() {
            bump();
        }
    });

    view! {
        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4">"Groupes"</h1>

        <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 mb-6 flex flex-col gap-3">
            <h2 class="text-base font-bold text-gray-900 dark:text-gray-100">"Créer un groupe"</h2>
            {move || form_error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Input label="Nom" value=new_name on_input=move |v| set_new_name.set(v)/>
                <Suspense fallback=|| view! { <p class="text-sm text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
                    {move || Suspend::new(async move {
                        let options = groups.await.unwrap_or_default();
                        let mut select_options = vec![SelectOption::new("", "— Groupe racine —")];
                        select_options.extend(options.into_iter().map(|g| SelectOption::new(g.id, g.name)));
                        view! {
                            <Select
                                label="Groupe parent"
                                options=select_options
                                value=new_parent
                                on_change=move |v| set_new_parent.set(v)
                            />
                        }
                    })}
                </Suspense>
            </div>
            <div>
                <Button
                    variant=ButtonVariant::Primary
                    disabled=create_action.pending().get()
                    on_click=move |_| {
                        if new_name.get().trim().is_empty() {
                            set_form_error.set(Some("Le nom du groupe est obligatoire.".to_string()));
                            return;
                        }
                        create_action.dispatch((new_name.get(), new_parent.get()));
                    }
                >
                    "Créer le groupe"
                </Button>
            </div>
        </div>

        {move || reparent_error.get().map(|message| view! {
            <Alert severity=Severity::Error small=true class="mb-3">{message}</Alert>
        })}

        <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement des groupes…"</p> }>
            {move || Suspend::new(async move {
                match groups.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => {
                        let select_options: Vec<SelectOption> = {
                            let mut options = vec![SelectOption::new("", "— Groupe racine —")];
                            options.extend(rows.iter().map(|g| SelectOption::new(g.id.clone(), g.name.clone())));
                            options
                        };
                        let flattened = flatten_tree(rows);
                        view! {
                            <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm divide-y divide-gray-200 dark:divide-gray-800">
                                {flattened.into_iter().map(|(group, depth)| {
                                    let group_id_for_rename = group.id.clone();
                                    let group_id_for_reparent = group.id.clone();
                                    let group_id_for_delete = group.id.clone();
                                    let group_id_for_details = group.id.clone();
                                    let parent_value = RwSignal::new(group.parent_id.clone().unwrap_or_default());
                                    let select_options = select_options.clone();
                                    view! {
                                        <div class="flex items-center gap-3 px-3 py-2" style=format!("padding-left: {}rem", 0.75 + depth as f64 * 1.5)>
                                            <span class="flex-1 min-w-0">
                                                <crate::component::InlineEditableField
                                                    value=Signal::derive(move || group.name.clone())
                                                    on_save=move |value: String| {
                                                        if !value.is_empty() {
                                                            rename_action.dispatch((group_id_for_rename.clone(), value));
                                                        }
                                                    }
                                                />
                                            </span>
                                            <Select
                                                label=""
                                                options=select_options
                                                value=parent_value
                                                class="w-56"
                                                on_change=move |value| {
                                                    parent_value.set(value.clone());
                                                    reparent_action.dispatch((group_id_for_reparent.clone(), value));
                                                }
                                            />
                                            <Button
                                                variant=ButtonVariant::TertiaryNoOutline
                                                size=dsfr::components::common::Size::Sm
                                                on_click=move |_| {
                                                    let current = selected_group.get_untracked();
                                                    if current.as_deref() == Some(group_id_for_details.as_str()) {
                                                        selected_group.set(None);
                                                    } else {
                                                        selected_group.set(Some(group_id_for_details.clone()));
                                                    }
                                                }
                                            >
                                                "Permissions"
                                            </Button>
                                            <ConfirmButton
                                                label="Supprimer"
                                                confirm_label="Confirmer ?"
                                                disabled=delete_action.pending().get()
                                                on_confirm=Callback::new(move |_| {
                                                    delete_action.dispatch(group_id_for_delete.clone());
                                                })
                                            />
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }
            })}
        </Suspense>

        {move || selected_group.get().map(|group_id| view! {
            <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 mt-4">
                <h3 class="text-base font-bold text-gray-900 dark:text-gray-100 mb-2">"Permissions du groupe"</h3>
                <PermissionsPanel subject_kind="group" subject_id=group_id is_super_admin=is_super_admin/>
            </div>
        })}
    }
}
