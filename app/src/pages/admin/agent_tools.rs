//! Page `/admin/agent-tools` : disponibilité des outils de l'agent IA par
//! domaine (ex. GéoRisques réservé à un domaine, Légifrance disponible
//! globalement) — voir `Claude.md` § Pages de l'application et
//! `agent::tools::CONFIGURABLE_TOOLS` pour le catalogue des outils
//! concernés (les outils cœur d'édition/interaction sont toujours
//! disponibles et n'apparaissent pas ici).

use std::collections::HashSet;

use dsfr::{Alert, Severity, Toggle};
use leptos::ev::Event;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DomainOption {
    id: String,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolRow {
    tool_name: String,
    label: String,
    is_global: bool,
    domain_ids: Vec<String>,
}

#[server]
async fn list_domains_for_agent_tools() -> Result<Vec<DomainOption>, ServerFnError> {
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
async fn list_agent_tools_admin() -> Result<Vec<ToolRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let scopes = storage::agent_tool_scope::list_agent_tool_scopes(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(agent::tools::CONFIGURABLE_TOOLS
        .iter()
        .map(|(tool_name, label)| {
            let is_global = scopes
                .iter()
                .any(|scope| scope.tool_name == *tool_name && scope.domain_id.is_none());
            let domain_ids = scopes
                .iter()
                .filter(|scope| scope.tool_name == *tool_name)
                .filter_map(|scope| scope.domain_id.as_ref().map(ToString::to_string))
                .collect();
            ToolRow {
                tool_name: tool_name.to_string(),
                label: label.to_string(),
                is_global,
                domain_ids,
            }
        })
        .collect())
}

#[server]
async fn set_agent_tool_global(tool_name: String, enabled: bool) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    storage::agent_tool_scope::set_tool_global(&pool, &tool_name, enabled)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "agent_tool_scope", None).await
}

#[server]
async fn set_agent_tool_domain(
    tool_name: String,
    domain_id: String,
    enabled: bool,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let domain_id: shared::id::ID = domain_id
        .parse()
        .map_err(|_| ServerFnError::new("domaine invalide"))?;

    storage::agent_tool_scope::set_tool_domain(&pool, &tool_name, &domain_id, enabled)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(
        &pool,
        actor_id,
        "update",
        "agent_tool_scope",
        Some(domain_id),
    )
    .await
}

#[component]
pub fn PageAdminAgentTools() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::AgentTools/>
                            <div class="max-w-6xl mx-auto p-6">
                                <AgentToolsPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn AgentToolsPanel() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);
    let tools = Resource::new(move || version.get(), |_| list_agent_tools_admin());
    let domains = Resource::new(move || version.get(), |_| list_domains_for_agent_tools());
    let (error, set_error) = signal(Option::<String>::None);

    let toggle_global = Action::new(|input: &(String, bool)| {
        let (tool_name, enabled) = input.clone();
        set_agent_tool_global(tool_name, enabled)
    });
    Effect::new(move |_| {
        if let Some(result) = toggle_global.value().get() {
            match result {
                Ok(()) => bump(),
                Err(error) => set_error.set(Some(error.to_string())),
            }
        }
    });

    let toggle_domain = Action::new(|input: &(String, String, bool)| {
        let (tool_name, domain_id, enabled) = input.clone();
        set_agent_tool_domain(tool_name, domain_id, enabled)
    });
    Effect::new(move |_| {
        if let Some(result) = toggle_domain.value().get() {
            match result {
                Ok(()) => bump(),
                Err(error) => set_error.set(Some(error.to_string())),
            }
        }
    });

    view! {
        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4">"Outils de l'agent"</h1>
        <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
            "Les outils cœur d'édition et d'interaction sont toujours disponibles. Les outils "
            "ci-dessous appellent des API externes : rendez-les disponibles globalement, ou "
            "réservez-les à certains domaines."
        </p>

        {move || error.get().map(|message| view! {
            <Alert severity=Severity::Error small=true class="mb-4">{message}</Alert>
        })}

        <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                let domain_options = domains.await.unwrap_or_default();
                match tools.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => view! {
                        <div class="flex flex-col gap-4">
                            {rows.into_iter().map(|tool| {
                                let domain_options = domain_options.clone();
                                let enabled_domains: HashSet<String> = tool.domain_ids.into_iter().collect();
                                let tool_name = tool.tool_name.clone();
                                let tool_name_for_global = tool_name.clone();
                                view! {
                                    <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 flex flex-col gap-3">
                                        <div class="flex items-center justify-between">
                                            <h2 class="text-base font-bold text-gray-900 dark:text-gray-100">{tool.label}</h2>
                                            <Toggle
                                                label="Disponible globalement"
                                                checked=tool.is_global
                                                on_toggle=move |enabled| {
                                                    toggle_global.dispatch((tool_name_for_global.clone(), enabled));
                                                }
                                            />
                                        </div>
                                        {(!domain_options.is_empty()).then(|| {
                                            let tool_name = tool_name.clone();
                                            view! {
                                                <div class="flex flex-wrap gap-4">
                                                    {domain_options.iter().map(|domain| {
                                                        let tool_name = tool_name.clone();
                                                        let domain_id = domain.id.clone();
                                                        let checked = enabled_domains.contains(&domain.id);
                                                        view! {
                                                            <label class="flex items-center gap-2 text-sm cursor-pointer">
                                                                <input
                                                                    type="checkbox"
                                                                    class="size-5 accent-blue-france cursor-pointer"
                                                                    prop:checked=checked
                                                                    on:change=move |ev: Event| {
                                                                        let enabled = event_target_checked(&ev);
                                                                        toggle_domain.dispatch((tool_name.clone(), domain_id.clone(), enabled));
                                                                    }
                                                                />
                                                                {domain.name.clone()}
                                                            </label>
                                                        }
                                                    }).collect::<Vec<_>>()}
                                                </div>
                                            }
                                        })}
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}
