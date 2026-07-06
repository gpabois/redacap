//! Page `/admin/users` : gestion des comptes utilisateurs et de leurs droits
//! (rattachement à des groupes, permissions directes) — voir `Claude.md`
//! § Pages de l'application.

use dsfr::{
    Alert, Badge, Button, ButtonVariant, Input, Select, SelectOption, Severity, Table, Tag, Toggle,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    AdminAccessDenied, AdminHeader, AdminNav, AdminSection, PermissionsPanel, admin_context,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupOption {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserRow {
    id: String,
    email: String,
    display_name: String,
    suspended: bool,
    groups: Vec<GroupOption>,
    is_administrator: bool,
    is_super_administrator: bool,
}

#[server]
async fn list_users_admin() -> Result<Vec<UserRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let users = storage::user::list_users(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let mut rows = Vec::with_capacity(users.len());
    for user in users {
        let group_ids = storage::user_group::list_groups_for_user(&pool, &user.id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
        let mut groups = Vec::with_capacity(group_ids.len());
        for group_id in group_ids {
            let group = storage::group::get_group(&pool, &group_id)
                .await
                .map_err(|error| ServerFnError::new(error.to_string()))?;
            groups.push(GroupOption {
                id: group.id.to_string(),
                name: group.name,
            });
        }
        let permissions = crate::auth::effective_permissions(&pool, &user.id).await?;
        let is_administrator =
            crate::auth::has_global_action(&permissions, shared::model::ACTION_ADMINISTRATEUR);
        let is_super_administrator = crate::auth::has_global_action(
            &permissions,
            shared::model::ACTION_SUPER_ADMINISTRATEUR,
        );
        rows.push(UserRow {
            id: user.id.to_string(),
            email: user.email,
            display_name: user.display_name,
            suspended: user.suspended_at.is_some(),
            groups,
            is_administrator,
            is_super_administrator,
        });
    }
    Ok(rows)
}

#[server]
async fn list_groups_for_select() -> Result<Vec<GroupOption>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;
    let groups = storage::group::list_all_groups(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(groups
        .into_iter()
        .map(|group| GroupOption {
            id: group.id.to_string(),
            name: group.name,
        })
        .collect())
}

#[server]
async fn create_user_admin(
    email: String,
    display_name: String,
    password: Option<String>,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let email = email.trim().to_string();
    let display_name = display_name.trim().to_string();
    if email.is_empty() || display_name.is_empty() {
        return Err(ServerFnError::new("email et nom sont obligatoires"));
    }

    let user = storage::user::create_user(
        &pool,
        shared::model::CreateUser {
            email,
            display_name,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    if let Some(password) = password.filter(|password| !password.is_empty()) {
        storage::credential::set_password(&pool, &user.id, &password)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
    }

    super::record_admin_audit_event(&pool, actor_id, "create", "user", Some(user.id)).await
}

#[server]
async fn rename_user_admin(user_id: String, display_name: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(ServerFnError::new("le nom ne peut pas être vide"));
    }
    let user_id: shared::id::ID = user_id
        .parse()
        .map_err(|_| ServerFnError::new("utilisateur invalide"))?;

    storage::user::update_user(
        &pool,
        &user_id,
        shared::model::UserChangeset {
            display_name: Some(display_name),
            ..Default::default()
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "user", Some(user_id)).await
}

#[server]
async fn set_user_suspended_admin(user_id: String, suspended: bool) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let user_id: shared::id::ID = user_id
        .parse()
        .map_err(|_| ServerFnError::new("utilisateur invalide"))?;

    if suspended {
        storage::user::suspend_user(&pool, &user_id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
        // Propagation immédiate de la révocation (voir contrainte racine
        // « Suspension de compte » : les sessions actives sont invalidées).
        storage::session::delete_sessions_for_user(&pool, &user_id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
        super::record_admin_audit_event(&pool, actor_id, "suspend", "user", Some(user_id)).await
    } else {
        storage::user::reactivate_user(&pool, &user_id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
        super::record_admin_audit_event(&pool, actor_id, "reactivate", "user", Some(user_id)).await
    }
}

#[server]
async fn delete_user_admin(user_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let user_id: shared::id::ID = user_id
        .parse()
        .map_err(|_| ServerFnError::new("utilisateur invalide"))?;
    if user_id == actor_id {
        return Err(ServerFnError::new(
            "vous ne pouvez pas supprimer votre propre compte",
        ));
    }

    storage::session::delete_sessions_for_user(&pool, &user_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    storage::user::delete_user(&pool, &user_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "delete", "user", Some(user_id)).await
}

#[server]
async fn assign_group_admin(user_id: String, group_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let user_id: shared::id::ID = user_id
        .parse()
        .map_err(|_| ServerFnError::new("utilisateur invalide"))?;
    let group_id: shared::id::ID = group_id
        .parse()
        .map_err(|_| ServerFnError::new("groupe invalide"))?;

    storage::user_group::add_user_to_group(&pool, &user_id, &group_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "grant", "user_group", Some(user_id)).await
}

#[server]
async fn unassign_group_admin(user_id: String, group_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let user_id: shared::id::ID = user_id
        .parse()
        .map_err(|_| ServerFnError::new("utilisateur invalide"))?;
    let group_id: shared::id::ID = group_id
        .parse()
        .map_err(|_| ServerFnError::new("groupe invalide"))?;

    storage::user_group::remove_user_from_group(&pool, &user_id, &group_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "revoke", "user_group", Some(user_id)).await
}

#[component]
pub fn PageAdminUsers() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(ctx) => view! {
                        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
                            <AdminHeader initial=ctx.initial.clone()/>
                            <AdminNav active=AdminSection::Users/>
                            <div class="max-w-6xl mx-auto p-6">
                                <UsersPanel is_super_admin=ctx.is_super_admin/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn UsersPanel(is_super_admin: bool) -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);

    let users = Resource::new(move || version.get(), |_| list_users_admin());
    let groups = Resource::new(move || version.get(), |_| list_groups_for_select());

    let (new_email, set_new_email) = signal(String::new());
    let (new_display_name, set_new_display_name) = signal(String::new());
    let (new_password, set_new_password) = signal(String::new());
    let (form_error, set_form_error) = signal(Option::<String>::None);

    let create_action = Action::new(move |input: &(String, String, String)| {
        let (email, display_name, password) = input.clone();
        let password = (!password.is_empty()).then_some(password);
        create_user_admin(email, display_name, password)
    });

    Effect::new(move |_| {
        if let Some(result) = create_action.value().get() {
            match result {
                Ok(()) => {
                    set_new_email.set(String::new());
                    set_new_display_name.set(String::new());
                    set_new_password.set(String::new());
                    set_form_error.set(None);
                    bump();
                }
                Err(error) => set_form_error.set(Some(error.to_string())),
            }
        }
    });

    let selected_user = RwSignal::new(Option::<String>::None);

    view! {
        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4">"Utilisateurs"</h1>

        <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 mb-6 flex flex-col gap-3">
            <h2 class="text-base font-bold text-gray-900 dark:text-gray-100">"Créer un compte"</h2>
            {move || form_error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
                <Input label="Email" r#type="email" value=new_email on_input=move |v| set_new_email.set(v)/>
                <Input label="Nom affiché" value=new_display_name on_input=move |v| set_new_display_name.set(v)/>
                <Input label="Mot de passe (optionnel)" r#type="password" value=new_password on_input=move |v| set_new_password.set(v)/>
            </div>
            <div>
                <Button
                    variant=ButtonVariant::Primary
                    disabled=create_action.pending().get()
                    on_click=move |_| {
                        if new_email.get().trim().is_empty() || new_display_name.get().trim().is_empty() {
                            set_form_error.set(Some("Email et nom sont obligatoires.".to_string()));
                            return;
                        }
                        create_action.dispatch((new_email.get(), new_display_name.get(), new_password.get()));
                    }
                >
                    "Créer le compte"
                </Button>
            </div>
        </div>

        <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement des utilisateurs…"</p> }>
            {move || Suspend::new(async move {
                match users.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => view! {
                        <Table headers=vec!["Email", "Nom", "Groupes", "Statut", "Droits", ""]>
                            {rows.into_iter().map(|user| {
                                let user_id = user.id.clone();
                                let user_id_for_toggle = user.id.clone();
                                let suspended = RwSignal::new(user.suspended);
                                view! {
                                    <tr>
                                        <td class="px-3 py-2">{user.email}</td>
                                        <td class="px-3 py-2">{user.display_name}</td>
                                        <td class="px-3 py-2">
                                            <div class="flex flex-wrap gap-1">
                                                {user.groups.into_iter().map(|g| view! {
                                                    <Tag on_click=|_| {}>{g.name}</Tag>
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        </td>
                                        <td class="px-3 py-2">
                                            <Toggle
                                                label=if suspended.get_untracked() { "Suspendu" } else { "Actif" }
                                                checked=suspended
                                                on_toggle=move |checked| {
                                                    suspended.set(checked);
                                                    let user_id = user_id_for_toggle.clone();
                                                    leptos::task::spawn_local(async move {
                                                        let _ = set_user_suspended_admin(user_id, checked).await;
                                                        bump();
                                                    });
                                                }
                                            />
                                        </td>
                                        <td class="px-3 py-2">
                                            <div class="flex gap-1">
                                                {user.is_super_administrator.then(|| view! {
                                                    <Badge severity=Severity::Warning small=true>"Super admin"</Badge>
                                                })}
                                                {(user.is_administrator && !user.is_super_administrator).then(|| view! {
                                                    <Badge severity=Severity::Info small=true>"Admin"</Badge>
                                                })}
                                            </div>
                                        </td>
                                        <td class="px-3 py-2">
                                            <Button
                                                variant=ButtonVariant::TertiaryNoOutline
                                                size=dsfr::components::common::Size::Sm
                                                on_click=move |_| {
                                                    let current = selected_user.get_untracked();
                                                    if current.as_deref() == Some(user_id.as_str()) {
                                                        selected_user.set(None);
                                                    } else {
                                                        selected_user.set(Some(user_id.clone()));
                                                    }
                                                }
                                            >
                                                "Détails"
                                            </Button>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </Table>
                    }.into_any(),
                }
            })}
        </Suspense>

        {move || selected_user.get().map(|user_id| view! {
            <UserDetailPanel
                user_id=user_id
                is_super_admin=is_super_admin
                groups=groups
                on_change=Callback::new(move |_: ()| bump())
            />
        })}
    }
}

#[component]
fn UserDetailPanel(
    user_id: String,
    is_super_admin: bool,
    groups: Resource<Result<Vec<GroupOption>, ServerFnError>>,
    on_change: Callback<()>,
) -> impl IntoView {
    let (group_to_add, set_group_to_add) = signal(String::new());
    let user_id_for_assign = user_id.clone();
    let assign_action = Action::new(move |group_id: &String| {
        assign_group_admin(user_id_for_assign.clone(), group_id.clone())
    });
    Effect::new(move |_| {
        if let Some(Ok(())) = assign_action.value().get() {
            set_group_to_add.set(String::new());
            on_change.run(());
        }
    });

    let user_id_for_unassign = user_id.clone();

    view! {
        <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 mt-4 flex flex-col gap-4">
            <div>
                <h3 class="text-base font-bold text-gray-900 dark:text-gray-100 mb-2">"Groupes"</h3>
                <Suspense fallback=|| view! { <p class="text-sm text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
                    {move || Suspend::new(async move {
                        let options = groups.await.unwrap_or_default();
                        let mut select_options = vec![SelectOption::new("", "— Ajouter à un groupe —")];
                        select_options.extend(options.into_iter().map(|g| SelectOption::new(g.id, g.name)));
                        view! {
                            <div class="flex items-end gap-2">
                                <Select
                                    label="Groupe"
                                    options=select_options
                                    value=group_to_add
                                    on_change=move |value| {
                                        set_group_to_add.set(value.clone());
                                        if !value.is_empty() {
                                            assign_action.dispatch(value);
                                        }
                                    }
                                />
                            </div>
                        }
                    })}
                </Suspense>
            </div>

            <div>
                <h3 class="text-base font-bold text-gray-900 dark:text-gray-100 mb-2">"Permissions"</h3>
                <PermissionsPanel subject_kind="user" subject_id=user_id.clone() is_super_admin=is_super_admin/>
            </div>

            <div class="flex flex-wrap gap-2">
                {move || {
                    let user_id_for_unassign = user_id_for_unassign.clone();
                    Suspend::new(async move {
                        let options = groups.await.unwrap_or_default();
                        options.into_iter().map(|group| {
                            let group_id = group.id.clone();
                            let user_id = user_id_for_unassign.clone();
                            view! {
                                <Tag on_click=move |_| {
                                    let user_id = user_id.clone();
                                    let group_id = group_id.clone();
                                    leptos::task::spawn_local(async move {
                                        let _ = unassign_group_admin(user_id, group_id).await;
                                    });
                                }>
                                    {format!("Retirer de {}", group.name)}
                                </Tag>
                            }
                        }).collect::<Vec<_>>()
                    })
                }}
            </div>

            <div>
                <DeleteUserButton user_id=user_id.clone() on_deleted=Callback::new(move |_: ()| on_change.run(()))/>
            </div>
        </div>
    }
}

#[component]
fn DeleteUserButton(user_id: String, on_deleted: Callback<()>) -> impl IntoView {
    let (confirming, set_confirming) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let delete_action = Action::new(move |user_id: &String| delete_user_admin(user_id.clone()));

    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => on_deleted.run(()),
                Err(error_value) => {
                    set_error.set(Some(error_value.to_string()));
                    set_confirming.set(false);
                }
            }
        }
    });

    view! {
        {move || error.get().map(|message| view! {
            <Alert severity=Severity::Error small=true>{message}</Alert>
        })}
        <Button
            variant=ButtonVariant::Secondary
            disabled=delete_action.pending().get()
            on_click=move |_| {
                if confirming.get() {
                    delete_action.dispatch(user_id.clone());
                } else {
                    set_confirming.set(true);
                }
            }
        >
            {move || if confirming.get() { "Confirmer la suppression ?" } else { "Supprimer le compte" }}
        </Button>
    }
}
