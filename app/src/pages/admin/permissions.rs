//! Composant partagé de gestion des permissions, utilisé par `/admin/users`
//! et `/admin/groups` : attribution de domaines, de droits sur des actes
//! légaux (arrêtés), et de portées génériques (ex. `administrateur`) à un
//! utilisateur ou à un groupe — voir `Claude.md` § Modèle de permissions.

use dsfr::{Alert, Button, ButtonVariant, Input, Select, SelectOption, Severity, Table};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PermissionOptionRow {
    id: String,
    label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PermissionRow {
    id: String,
    resource_type: String,
    resource_label: String,
    action: String,
}

#[cfg(feature = "ssr")]
fn parse_subject(
    subject_kind: &str,
    subject_id: &str,
) -> Result<shared::model::Subject, ServerFnError> {
    let subject_id: shared::id::ID = subject_id
        .parse()
        .map_err(|_| ServerFnError::new("titulaire invalide"))?;
    match subject_kind {
        "user" => Ok(shared::model::Subject::User(subject_id)),
        "group" => Ok(shared::model::Subject::Group(subject_id)),
        _ => Err(ServerFnError::new("type de titulaire invalide")),
    }
}

/// Résout un libellé lisible pour la ressource ciblée par une permission :
/// nom du domaine ou titre de l'acte légal quand la ressource est connue,
/// sinon un libellé générique par identifiant (portée `Global`/`ManagedByGroup`
/// ou type de ressource non résolu ici).
#[cfg(feature = "ssr")]
async fn resolve_resource_label(
    pool: &storage::Pool,
    permission: &shared::model::Permission,
) -> String {
    match (permission.resource_type.as_str(), permission.resource) {
        ("domain", shared::model::ResourceScope::Specific(id)) => {
            storage::domain::get_domain(pool, &id)
                .await
                .map(|domain| domain.name)
                .unwrap_or_else(|_| format!("Domaine {id}"))
        }
        ("legal_act", shared::model::ResourceScope::Specific(id)) => {
            storage::legal_act::get_legal_act(pool, &id)
                .await
                .map(|legal_act| legal_act.title)
                .unwrap_or_else(|_| format!("Acte légal {id}"))
        }
        (_, shared::model::ResourceScope::Global) => "Global".to_string(),
        (_, shared::model::ResourceScope::Specific(id)) => format!("Ressource {id}"),
        (_, shared::model::ResourceScope::ManagedByGroup(id)) => {
            format!("Géré par le groupe {id}")
        }
    }
}

#[server]
async fn list_domain_options_admin() -> Result<Vec<PermissionOptionRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let domains = storage::domain::list_domains(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(domains
        .into_iter()
        .map(|domain| PermissionOptionRow {
            id: domain.id.to_string(),
            label: domain.name,
        })
        .collect())
}

#[server]
async fn list_legal_act_options_admin() -> Result<Vec<PermissionOptionRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let legal_acts = storage::legal_act::list_all_legal_acts(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(legal_acts
        .into_iter()
        .map(|legal_act| PermissionOptionRow {
            id: legal_act.id.to_string(),
            label: legal_act.title,
        })
        .collect())
}

#[server]
async fn list_permissions_for_subject_admin(
    subject_kind: String,
    subject_id: String,
) -> Result<Vec<PermissionRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let subject_id: shared::id::ID = subject_id
        .parse()
        .map_err(|_| ServerFnError::new("titulaire invalide"))?;
    let permissions = match subject_kind.as_str() {
        "user" => storage::permission::list_permissions_for_user(&pool, &subject_id).await,
        "group" => storage::permission::list_permissions_for_group(&pool, &subject_id).await,
        _ => return Err(ServerFnError::new("type de titulaire invalide")),
    }
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    let mut rows = Vec::with_capacity(permissions.len());
    for permission in permissions {
        let resource_label = resolve_resource_label(&pool, &permission).await;
        rows.push(PermissionRow {
            id: permission.id.to_string(),
            resource_type: permission.resource_type,
            resource_label,
            action: permission.action,
        });
    }
    Ok(rows)
}

#[server]
async fn grant_domain_permission_admin(
    subject_kind: String,
    subject_id: String,
    domain_id: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let subject = parse_subject(&subject_kind, &subject_id)?;
    let domain_id: shared::id::ID = domain_id
        .parse()
        .map_err(|_| ServerFnError::new("domaine invalide"))?;

    let permission = storage::permission::create_permission(
        &pool,
        shared::model::CreatePermission {
            subject,
            resource_type: "domain".to_string(),
            resource: shared::model::ResourceScope::Specific(domain_id),
            action: "use".to_string(),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "grant", "permission", Some(permission.id))
        .await
}

#[server]
async fn grant_legal_act_permission_admin(
    subject_kind: String,
    subject_id: String,
    legal_act_id: String,
    action: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let action = action.trim().to_string();
    if action.is_empty() {
        return Err(ServerFnError::new("l'action est obligatoire"));
    }
    let subject = parse_subject(&subject_kind, &subject_id)?;
    let legal_act_id: shared::id::ID = legal_act_id
        .parse()
        .map_err(|_| ServerFnError::new("acte légal invalide"))?;

    let permission = storage::permission::create_permission(
        &pool,
        shared::model::CreatePermission {
            subject,
            resource_type: "legal_act".to_string(),
            resource: shared::model::ResourceScope::Specific(legal_act_id),
            action,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "grant", "permission", Some(permission.id))
        .await
}

#[server]
async fn grant_generic_permission_admin(
    subject_kind: String,
    subject_id: String,
    resource_type: String,
    resource_kind: String,
    resource_id: Option<String>,
    action: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let action = action.trim().to_string();
    let resource_type = resource_type.trim().to_string();
    if action.is_empty() || resource_type.is_empty() {
        return Err(ServerFnError::new(
            "le type de ressource et l'action sont obligatoires",
        ));
    }
    if crate::auth::is_admin_tier_action(&action) {
        crate::auth::require_super_admin(&pool, &actor_id).await?;
    }

    let subject = parse_subject(&subject_kind, &subject_id)?;

    let resource = match resource_kind.as_str() {
        "global" => shared::model::ResourceScope::Global,
        "specific" => {
            let id: shared::id::ID = resource_id
                .ok_or_else(|| ServerFnError::new("identifiant de ressource requis"))?
                .parse()
                .map_err(|_| ServerFnError::new("identifiant de ressource invalide"))?;
            shared::model::ResourceScope::Specific(id)
        }
        "managed_by_group" => {
            let id: shared::id::ID = resource_id
                .ok_or_else(|| ServerFnError::new("groupe gestionnaire requis"))?
                .parse()
                .map_err(|_| ServerFnError::new("groupe gestionnaire invalide"))?;
            shared::model::ResourceScope::ManagedByGroup(id)
        }
        _ => return Err(ServerFnError::new("portée de ressource invalide")),
    };

    let permission = storage::permission::create_permission(
        &pool,
        shared::model::CreatePermission {
            subject,
            resource_type,
            resource,
            action,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "grant", "permission", Some(permission.id))
        .await
}

#[server]
async fn revoke_permission_admin(permission_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let permission_id: shared::id::ID = permission_id
        .parse()
        .map_err(|_| ServerFnError::new("permission invalide"))?;
    let permission = storage::permission::get_permission(&pool, &permission_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    if crate::auth::is_admin_tier_action(&permission.action) {
        crate::auth::require_super_admin(&pool, &actor_id).await?;
    }

    storage::permission::delete_permission(&pool, &permission_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "revoke", "permission", Some(permission_id))
        .await
}

/// Panneau de gestion des permissions d'un titulaire (utilisateur ou
/// groupe) : attribution de domaines (action `use`), de droits sur des
/// actes légaux (action libre, `edit` par défaut), et — dans une section
/// repliée — de portées génériques pour les cas non couverts par les deux
/// premières (ex. droits `administrateur`/`super_administrateur` à portée
/// `Global` sur `resource_type = "application"`).
#[component]
pub fn PermissionsPanel(
    subject_kind: &'static str,
    #[prop(into)] subject_id: String,
    is_super_admin: bool,
) -> impl IntoView {
    let version = RwSignal::new(0u32);

    let subject_id_for_perms = subject_id.clone();
    let permissions = Resource::new(
        move || (subject_kind, subject_id_for_perms.clone(), version.get()),
        |(kind, id, _)| list_permissions_for_subject_admin(kind.to_string(), id),
    );
    let domain_options = Resource::new(|| (), |_| list_domain_options_admin());
    let legal_act_options = Resource::new(|| (), |_| list_legal_act_options_admin());

    let (domain_to_grant, set_domain_to_grant) = signal(String::new());
    let (domain_error, set_domain_error) = signal(Option::<String>::None);
    let subject_id_for_domain = subject_id.clone();
    let grant_domain_action = Action::new(move |domain_id: &String| {
        grant_domain_permission_admin(
            subject_kind.to_string(),
            subject_id_for_domain.clone(),
            domain_id.clone(),
        )
    });
    Effect::new(move |_| {
        if let Some(result) = grant_domain_action.value().get() {
            match result {
                Ok(()) => {
                    set_domain_to_grant.set(String::new());
                    set_domain_error.set(None);
                    version.update(|v| *v += 1);
                }
                Err(error) => set_domain_error.set(Some(error.to_string())),
            }
        }
    });

    let (legal_act_to_grant, set_legal_act_to_grant) = signal(String::new());
    let (legal_act_action, set_legal_act_action) = signal("edit".to_string());
    let (legal_act_error, set_legal_act_error) = signal(Option::<String>::None);
    let subject_id_for_legal_act = subject_id.clone();
    let grant_legal_act_action = Action::new(move |input: &(String, String)| {
        let (legal_act_id, action) = input.clone();
        grant_legal_act_permission_admin(
            subject_kind.to_string(),
            subject_id_for_legal_act.clone(),
            legal_act_id,
            action,
        )
    });
    Effect::new(move |_| {
        if let Some(result) = grant_legal_act_action.value().get() {
            match result {
                Ok(()) => {
                    set_legal_act_to_grant.set(String::new());
                    set_legal_act_error.set(None);
                    version.update(|v| *v += 1);
                }
                Err(error) => set_legal_act_error.set(Some(error.to_string())),
            }
        }
    });

    let (resource_type, set_resource_type) = signal(String::new());
    let (resource_kind, set_resource_kind) = signal("global".to_string());
    let (resource_id, set_resource_id) = signal(String::new());
    let (generic_action, set_generic_action) = signal(String::new());
    let (generic_error, set_generic_error) = signal(Option::<String>::None);
    let subject_id_for_generic = subject_id.clone();
    let grant_generic_action = Action::new(move |input: &(String, String, String, String)| {
        let (resource_type, resource_kind, resource_id, action) = input.clone();
        grant_generic_permission_admin(
            subject_kind.to_string(),
            subject_id_for_generic.clone(),
            resource_type,
            resource_kind,
            (!resource_id.is_empty()).then_some(resource_id),
            action,
        )
    });
    Effect::new(move |_| {
        if let Some(result) = grant_generic_action.value().get() {
            match result {
                Ok(()) => {
                    set_resource_type.set(String::new());
                    set_resource_id.set(String::new());
                    set_generic_action.set(String::new());
                    set_generic_error.set(None);
                    version.update(|v| *v += 1);
                }
                Err(error) => set_generic_error.set(Some(error.to_string())),
            }
        }
    });

    let revoke_action =
        Action::new(|permission_id: &String| revoke_permission_admin(permission_id.clone()));
    Effect::new(move |_| {
        if let Some(Ok(())) = revoke_action.value().get() {
            version.update(|v| *v += 1);
        }
    });

    view! {
        <div class="flex flex-col gap-4">
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div class="border border-gray-200 rounded-sm p-3">
                    <h4 class="text-sm font-bold text-gray-900 mb-2">"Domaines"</h4>
                    {move || domain_error.get().map(|message| view! {
                        <Alert severity=Severity::Error small=true>{message}</Alert>
                    })}
                    <Suspense fallback=|| view! { <p class="text-sm text-gray-500">"Chargement…"</p> }>
                        {move || Suspend::new(async move {
                            let options = domain_options.await.unwrap_or_default();
                            let mut select_options = vec![SelectOption::new("", "— Choisir un domaine —")];
                            select_options.extend(options.into_iter().map(|o| SelectOption::new(o.id, o.label)));
                            view! {
                                <Select
                                    label="Domaine à accorder"
                                    options=select_options
                                    value=domain_to_grant
                                    on_change=move |value| {
                                        set_domain_to_grant.set(value.clone());
                                        if !value.is_empty() {
                                            grant_domain_action.dispatch(value);
                                        }
                                    }
                                />
                            }
                        })}
                    </Suspense>
                </div>

                <div class="border border-gray-200 rounded-sm p-3">
                    <h4 class="text-sm font-bold text-gray-900 mb-2">"Actes légaux (arrêtés)"</h4>
                    {move || legal_act_error.get().map(|message| view! {
                        <Alert severity=Severity::Error small=true>{message}</Alert>
                    })}
                    <Suspense fallback=|| view! { <p class="text-sm text-gray-500">"Chargement…"</p> }>
                        {move || Suspend::new(async move {
                            let options = legal_act_options.await.unwrap_or_default();
                            let mut select_options = vec![SelectOption::new("", "— Choisir un acte —")];
                            select_options.extend(options.into_iter().map(|o| SelectOption::new(o.id, o.label)));
                            view! {
                                <div class="flex flex-col gap-2">
                                    <Select
                                        label="Acte légal"
                                        options=select_options
                                        value=legal_act_to_grant
                                        on_change=move |value| set_legal_act_to_grant.set(value)
                                    />
                                    <Input
                                        label="Action"
                                        value=legal_act_action
                                        on_input=move |v| set_legal_act_action.set(v)
                                    />
                                    <div>
                                        <Button
                                            variant=ButtonVariant::Secondary
                                            disabled=grant_legal_act_action.pending().get()
                                            on_click=move |_| {
                                                let legal_act_id = legal_act_to_grant.get_untracked();
                                                if legal_act_id.is_empty() {
                                                    set_legal_act_error.set(Some("Choisissez un acte légal.".to_string()));
                                                    return;
                                                }
                                                grant_legal_act_action.dispatch((legal_act_id, legal_act_action.get_untracked()));
                                            }
                                        >
                                            "Accorder"
                                        </Button>
                                    </div>
                                </div>
                            }
                        })}
                    </Suspense>
                </div>
            </div>

            <div>
                <h4 class="text-sm font-bold text-gray-900 mb-2">"Permissions accordées"</h4>
                <Suspense fallback=|| view! { <p class="text-sm text-gray-500">"Chargement…"</p> }>
                    {move || Suspend::new(async move {
                        match permissions.await {
                            Err(error) => view! { <Alert severity=Severity::Error small=true>{error.to_string()}</Alert> }.into_any(),
                            Ok(rows) if rows.is_empty() => view! {
                                <p class="text-sm text-gray-500">"Aucune permission directe."</p>
                            }.into_any(),
                            Ok(rows) => view! {
                                <Table headers=vec!["Type de ressource", "Ressource", "Action", ""]>
                                    {rows.into_iter().map(|permission| {
                                        let permission_id = permission.id.clone();
                                        view! {
                                            <tr>
                                                <td class="px-3 py-2">{permission.resource_type}</td>
                                                <td class="px-3 py-2">{permission.resource_label}</td>
                                                <td class="px-3 py-2">{permission.action}</td>
                                                <td class="px-3 py-2">
                                                    <Button
                                                        variant=ButtonVariant::TertiaryNoOutline
                                                        size=dsfr::components::common::Size::Sm
                                                        disabled=revoke_action.pending().get()
                                                        on_click=move |_| { revoke_action.dispatch(permission_id.clone()); }
                                                    >
                                                        "Révoquer"
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
            </div>

            <details class="border border-gray-200 rounded-sm p-3">
                <summary class="text-sm font-bold text-gray-900 cursor-pointer">
                    "Portée avancée (application, gestion par groupe…)"
                </summary>
                <div class="mt-3 flex flex-col gap-3">
                    {move || generic_error.get().map(|message| view! {
                        <Alert severity=Severity::Error small=true>{message}</Alert>
                    })}
                    <div class="grid grid-cols-1 md:grid-cols-4 gap-3">
                        <Input label="Type de ressource" value=resource_type on_input=move |v| set_resource_type.set(v)/>
                        <Select
                            label="Portée"
                            options=vec![
                                SelectOption::new("global", "Global"),
                                SelectOption::new("specific", "Ressource précise"),
                                SelectOption::new("managed_by_group", "Géré par un groupe"),
                            ]
                            value=resource_kind
                            on_change=move |v| set_resource_kind.set(v)
                        />
                        <Input
                            label="Identifiant de ressource"
                            value=resource_id
                            disabled=resource_kind.get() == "global"
                            on_input=move |v| set_resource_id.set(v)
                        />
                        <Input label="Action" value=generic_action on_input=move |v| set_generic_action.set(v)/>
                    </div>
                    {(!is_super_admin).then(|| view! {
                        <p class="text-xs text-gray-500">
                            "Seul un super administrateur peut accorder ou révoquer les actions "
                            <code>"administrateur"</code>" / "<code>"super_administrateur"</code>"."
                        </p>
                    })}
                    <div>
                        <Button
                            variant=ButtonVariant::Secondary
                            disabled=grant_generic_action.pending().get()
                            on_click=move |_| {
                                grant_generic_action.dispatch((
                                    resource_type.get(),
                                    resource_kind.get(),
                                    resource_id.get(),
                                    generic_action.get(),
                                ));
                            }
                        >
                            "Accorder"
                        </Button>
                    </div>
                </div>
            </details>
        </div>
    }
}
