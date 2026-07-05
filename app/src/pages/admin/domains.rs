//! Page `/admin/domains` : gestion des domaines techniques d'un acte légal
//! (ex. « Installation classée »), gérés par les administrateurs — voir
//! `Claude.md` § Pages de l'application. Chaque domaine injecte son
//! `agent_context` en complément du prompt système de l'agent IA pour les
//! projets qui lui appartiennent.

use dsfr::{Alert, Button, ButtonVariant, Input, Severity, Table, Textarea};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, ConfirmButton, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DomainRow {
    id: String,
    name: String,
    agent_context: String,
}

#[server]
async fn list_domains_admin() -> Result<Vec<DomainRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let domains = storage::domain::list_domains(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(domains
        .into_iter()
        .map(|domain| DomainRow {
            id: domain.id.to_string(),
            name: domain.name,
            agent_context: domain.agent_context,
        })
        .collect())
}

#[server]
async fn create_domain_admin(name: String, agent_context: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("le nom du domaine est obligatoire"));
    }

    let domain = storage::domain::create_domain(
        &pool,
        shared::model::CreateDomain {
            name,
            agent_context: agent_context.trim().to_string(),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "create", "domain", Some(domain.id)).await
}

#[server]
async fn update_domain_admin(
    domain_id: String,
    name: String,
    agent_context: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("le nom du domaine est obligatoire"));
    }
    let domain_id: shared::id::ID = domain_id
        .parse()
        .map_err(|_| ServerFnError::new("domaine invalide"))?;

    storage::domain::update_domain(
        &pool,
        &domain_id,
        shared::model::DomainChangeset {
            name: Some(name),
            agent_context: Some(agent_context.trim().to_string()),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "domain", Some(domain_id)).await
}

#[server]
async fn delete_domain_admin(domain_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let domain_id: shared::id::ID = domain_id
        .parse()
        .map_err(|_| ServerFnError::new("domaine invalide"))?;

    storage::domain::delete_domain(&pool, &domain_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "delete", "domain", Some(domain_id)).await
}

#[component]
pub fn PageAdminDomains() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::Domains/>
                            <div class="max-w-6xl mx-auto p-6">
                                <DomainsPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn DomainsPanel() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);
    let domains = Resource::new(move || version.get(), |_| list_domains_admin());

    let (name, set_name) = signal(String::new());
    let (agent_context, set_agent_context) = signal(String::new());
    let (form_error, set_form_error) = signal(Option::<String>::None);
    let editing_id = RwSignal::new(Option::<String>::None);

    let reset_form = move || {
        set_name.set(String::new());
        set_agent_context.set(String::new());
        set_form_error.set(None);
        editing_id.set(None);
    };

    let create_action = Action::new(move |input: &(String, String)| {
        let (name, agent_context) = input.clone();
        create_domain_admin(name, agent_context)
    });
    Effect::new(move |_| {
        if let Some(result) = create_action.value().get() {
            match result {
                Ok(()) => {
                    reset_form();
                    bump();
                }
                Err(error) => set_form_error.set(Some(error.to_string())),
            }
        }
    });

    let update_action = Action::new(move |input: &(String, String, String)| {
        let (id, name, agent_context) = input.clone();
        update_domain_admin(id, name, agent_context)
    });
    Effect::new(move |_| {
        if let Some(result) = update_action.value().get() {
            match result {
                Ok(()) => {
                    reset_form();
                    bump();
                }
                Err(error) => set_form_error.set(Some(error.to_string())),
            }
        }
    });

    let delete_action = Action::new(|id: &String| delete_domain_admin(id.clone()));
    let (delete_error, set_delete_error) = signal(Option::<String>::None);
    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => {
                    set_delete_error.set(None);
                    bump();
                }
                Err(error) => set_delete_error.set(Some(error.to_string())),
            }
        }
    });

    view! {
        <h1 class="text-xl font-bold text-gray-900 mb-4">"Domaines techniques"</h1>

        <div class="bg-white border border-gray-200 rounded-sm p-4 mb-6 flex flex-col gap-3">
            <h2 class="text-base font-bold text-gray-900">
                {move || if editing_id.get().is_some() { "Modifier le domaine" } else { "Créer un domaine" }}
            </h2>
            {move || form_error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <div class="flex flex-col gap-3">
                <Input label="Nom" value=name on_input=move |v| set_name.set(v)/>
                <Textarea
                    label="Contexte pour l'agent IA"
                    hint="Texte ajouté au prompt système de l'agent pour tout projet de ce domaine."
                    value=agent_context
                    on_input=move |v| set_agent_context.set(v)
                />
            </div>
            <div class="flex gap-2">
                <Button
                    variant=ButtonVariant::Primary
                    disabled=create_action.pending().get() || update_action.pending().get()
                    on_click=move |_| {
                        if name.get().trim().is_empty() {
                            set_form_error.set(Some("Le nom est obligatoire.".to_string()));
                            return;
                        }
                        match editing_id.get_untracked() {
                            Some(id) => {
                                update_action.dispatch((id, name.get(), agent_context.get()));
                            }
                            None => {
                                create_action.dispatch((name.get(), agent_context.get()));
                            }
                        }
                    }
                >
                    {move || if editing_id.get().is_some() { "Enregistrer les modifications" } else { "Créer" }}
                </Button>
                {move || editing_id.get().is_some().then(|| {
                    view! {
                        <Button variant=ButtonVariant::Tertiary on_click=move |_| reset_form()>
                            "Annuler"
                        </Button>
                    }
                })}
            </div>
        </div>

        {move || delete_error.get().map(|message| view! {
            <Alert severity=Severity::Error small=true class="mb-3">{message}</Alert>
        })}

        <Suspense fallback=|| view! { <p class="text-gray-500">"Chargement des domaines…"</p> }>
            {move || Suspend::new(async move {
                match domains.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => view! {
                        <Table headers=vec!["Nom", "Contexte agent", ""]>
                            {rows.into_iter().map(|domain| {
                                let domain_id = domain.id.clone();
                                let edit_snapshot = (domain.id.clone(), domain.name.clone(), domain.agent_context.clone());
                                view! {
                                    <tr>
                                        <td class="px-3 py-2">{domain.name}</td>
                                        <td class="px-3 py-2 text-gray-600 max-w-md truncate">{domain.agent_context}</td>
                                        <td class="px-3 py-2">
                                            <div class="flex gap-2">
                                                <Button
                                                    variant=ButtonVariant::TertiaryNoOutline
                                                    on_click=move |_| {
                                                        let (id, name, agent_context) = edit_snapshot.clone();
                                                        set_name.set(name);
                                                        set_agent_context.set(agent_context);
                                                        set_form_error.set(None);
                                                        editing_id.set(Some(id));
                                                    }
                                                >
                                                    "Éditer"
                                                </Button>
                                                <ConfirmButton
                                                    label="Supprimer"
                                                    confirm_label="Confirmer ?"
                                                    disabled=delete_action.pending().get()
                                                    on_confirm=Callback::new(move |_| {
                                                        delete_action.dispatch(domain_id.clone());
                                                    })
                                                />
                                            </div>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </Table>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}
