//! Page `/admin/intentions` : gestion des intentions rédactionnelles d'un
//! acte légal (ex. « mise en demeure », « sanction administrative »),
//! rattachées à un domaine — voir `Claude.md` § Pages de l'application.
//! Seules les intentions du domaine d'un projet peuvent lui être associées
//! (voir `app::pages::project_intentions`), et chacune injecte son
//! `agent_context` en complément du prompt système de l'agent IA.

use dsfr::{Alert, Button, ButtonVariant, Input, Select, SelectOption, Severity, Table, Textarea};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, ConfirmButton, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DomainOption {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IntentionRow {
    id: String,
    domain_id: String,
    domain_name: String,
    name: String,
    agent_context: String,
}

#[server]
async fn list_domains_for_intention_select() -> Result<Vec<DomainOption>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let domains = storage::domain::list_domains(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(domains
        .into_iter()
        .map(|domain| DomainOption {
            id: domain.id.to_string(),
            name: domain.name,
        })
        .collect())
}

#[server]
async fn list_intentions_admin() -> Result<Vec<IntentionRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let domains = storage::domain::list_domains(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let mut rows = Vec::new();
    for domain in domains {
        let intentions = storage::intention::list_intentions_by_domain(&pool, &domain.id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
        for intention in intentions {
            rows.push(IntentionRow {
                id: intention.id.to_string(),
                domain_id: domain.id.to_string(),
                domain_name: domain.name.clone(),
                name: intention.name,
                agent_context: intention.agent_context,
            });
        }
    }
    Ok(rows)
}

#[server]
async fn create_intention_admin(
    domain_id: String,
    name: String,
    agent_context: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    if domain_id.is_empty() || name.is_empty() {
        return Err(ServerFnError::new(
            "le domaine et le nom de l'intention sont obligatoires",
        ));
    }
    let domain_id: shared::id::ID = domain_id
        .parse()
        .map_err(|_| ServerFnError::new("domaine invalide"))?;

    let intention = storage::intention::create_intention(
        &pool,
        shared::model::CreateIntention {
            domain_id,
            name,
            agent_context: agent_context.trim().to_string(),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "create", "intention", Some(intention.id))
        .await
}

#[server]
async fn update_intention_admin(
    intention_id: String,
    domain_id: String,
    name: String,
    agent_context: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("le nom de l'intention est obligatoire"));
    }
    let intention_id: shared::id::ID = intention_id
        .parse()
        .map_err(|_| ServerFnError::new("intention invalide"))?;
    let domain_id: shared::id::ID = domain_id
        .parse()
        .map_err(|_| ServerFnError::new("domaine invalide"))?;

    storage::intention::update_intention(
        &pool,
        &intention_id,
        shared::model::IntentionChangeset {
            domain_id: Some(domain_id),
            name: Some(name),
            agent_context: Some(agent_context.trim().to_string()),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "intention", Some(intention_id))
        .await
}

#[server]
async fn delete_intention_admin(intention_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let intention_id: shared::id::ID = intention_id
        .parse()
        .map_err(|_| ServerFnError::new("intention invalide"))?;

    storage::intention::delete_intention(&pool, &intention_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "delete", "intention", Some(intention_id))
        .await
}

#[component]
pub fn PageAdminIntentions() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::Intentions/>
                            <div class="max-w-6xl mx-auto p-6">
                                <IntentionsPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn IntentionsPanel() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);
    let intentions = Resource::new(move || version.get(), |_| list_intentions_admin());
    let domains = Resource::new(
        move || version.get(),
        |_| list_domains_for_intention_select(),
    );

    let (domain_id, set_domain_id) = signal(String::new());
    let (name, set_name) = signal(String::new());
    let (agent_context, set_agent_context) = signal(String::new());
    let (form_error, set_form_error) = signal(Option::<String>::None);
    let editing_id = RwSignal::new(Option::<String>::None);

    let reset_form = move || {
        set_domain_id.set(String::new());
        set_name.set(String::new());
        set_agent_context.set(String::new());
        set_form_error.set(None);
        editing_id.set(None);
    };

    let create_action = Action::new(move |input: &(String, String, String)| {
        let (domain_id, name, agent_context) = input.clone();
        create_intention_admin(domain_id, name, agent_context)
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

    let update_action = Action::new(move |input: &(String, String, String, String)| {
        let (id, domain_id, name, agent_context) = input.clone();
        update_intention_admin(id, domain_id, name, agent_context)
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

    let delete_action = Action::new(|id: &String| delete_intention_admin(id.clone()));
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
        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4">"Intentions"</h1>

        <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 mb-6 flex flex-col gap-3">
            <h2 class="text-base font-bold text-gray-900 dark:text-gray-100">
                {move || if editing_id.get().is_some() { "Modifier l'intention" } else { "Créer une intention" }}
            </h2>
            {move || form_error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Suspense fallback=|| view! { <p class="text-sm text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
                    {move || Suspend::new(async move {
                        let options = domains.await.unwrap_or_default();
                        let mut select_options = vec![SelectOption::new("", "— Sélectionner un domaine —")];
                        select_options.extend(options.into_iter().map(|d| SelectOption::new(d.id, d.name)));
                        view! {
                            <Select
                                label="Domaine"
                                options=select_options
                                value=domain_id
                                on_change=move |v| set_domain_id.set(v)
                            />
                        }
                    })}
                </Suspense>
                <Input label="Nom" value=name on_input=move |v| set_name.set(v)/>
                <Textarea
                    label="Contexte pour l'agent IA"
                    hint="Texte ajouté au prompt système lorsque cette intention est associée à un projet."
                    value=agent_context
                    on_input=move |v| set_agent_context.set(v)
                    class="md:col-span-2"
                />
            </div>
            <div class="flex gap-2">
                <Button
                    variant=ButtonVariant::Primary
                    disabled=create_action.pending().get() || update_action.pending().get()
                    on_click=move |_| {
                        if domain_id.get().is_empty() || name.get().trim().is_empty() {
                            set_form_error.set(Some("Domaine et nom sont obligatoires.".to_string()));
                            return;
                        }
                        match editing_id.get_untracked() {
                            Some(id) => {
                                update_action.dispatch((id, domain_id.get(), name.get(), agent_context.get()));
                            }
                            None => {
                                create_action.dispatch((domain_id.get(), name.get(), agent_context.get()));
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

        <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement des intentions…"</p> }>
            {move || Suspend::new(async move {
                match intentions.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => view! {
                        <Table headers=vec!["Domaine", "Nom", "Contexte agent", ""]>
                            {rows.into_iter().map(|intention| {
                                let delete_id = intention.id.clone();
                                let edit_snapshot = (
                                    intention.id.clone(),
                                    intention.domain_id.clone(),
                                    intention.name.clone(),
                                    intention.agent_context.clone(),
                                );
                                view! {
                                    <tr>
                                        <td class="px-3 py-2">{intention.domain_name}</td>
                                        <td class="px-3 py-2">{intention.name}</td>
                                        <td class="px-3 py-2 text-gray-600 dark:text-gray-400 max-w-md truncate">{intention.agent_context}</td>
                                        <td class="px-3 py-2">
                                            <div class="flex gap-2">
                                                <Button
                                                    variant=ButtonVariant::TertiaryNoOutline
                                                    on_click=move |_| {
                                                        let (id, domain_id, name, agent_context) = edit_snapshot.clone();
                                                        set_domain_id.set(domain_id);
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
                                                        delete_action.dispatch(delete_id.clone());
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
