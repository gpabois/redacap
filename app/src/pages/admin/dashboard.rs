//! Page `/admin` : tableau de bord du panneau administrateur — vue
//! d'ensemble des effectifs et raccourcis vers chaque section, voir
//! `Claude.md` § Pages de l'application.

use dsfr::{Alert, Severity, Tile};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecentAuditEntry {
    occurred_at: String,
    actor: String,
    action: String,
    resource_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DashboardStats {
    user_count: usize,
    group_count: usize,
    authority_count: usize,
    domain_count: usize,
    active_oidc_provider_count: usize,
    recent_audit_events: Vec<RecentAuditEntry>,
}

#[server]
async fn admin_dashboard_stats() -> Result<DashboardStats, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let users = storage::user::list_users(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let groups = storage::group::list_all_groups(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let authorities = storage::authority::list_authorities(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let domain_count = storage::domain::list_domains(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?
        .len();
    let oidc_providers = storage::oidc_provider::list_active_oidc_providers(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let recent_entries = storage::audit_log::list_audit_events(&pool, None, 5, 0)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let mut recent_audit_events = Vec::with_capacity(recent_entries.len());
    for entry in recent_entries {
        let actor = match entry.actor_id {
            Some(entry_actor_id) => storage::user::get_user(&pool, &entry_actor_id)
                .await
                .map(|user| user.display_name)
                .unwrap_or_else(|_| "Utilisateur supprimé".to_string()),
            None => "Système".to_string(),
        };
        recent_audit_events.push(RecentAuditEntry {
            occurred_at: entry.occurred_at.format("%d/%m/%Y %H:%M:%S").to_string(),
            actor,
            action: entry.action,
            resource_type: entry.resource_type,
        });
    }

    Ok(DashboardStats {
        user_count: users.len(),
        group_count: groups.len(),
        authority_count: authorities.len(),
        domain_count,
        active_oidc_provider_count: oidc_providers.len(),
        recent_audit_events,
    })
}

#[component]
pub fn PageAdminDashboard() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::Dashboard/>
                            <div class="max-w-6xl mx-auto p-6">
                                <DashboardPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn StatCard(label: &'static str, value: usize) -> impl IntoView {
    view! {
        <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 flex flex-col gap-1">
            <span class="text-3xl font-bold text-blue-france dark:text-blue-france-925">{value}</span>
            <span class="text-sm text-gray-700 dark:text-gray-300">{label}</span>
        </div>
    }
}

#[component]
fn DashboardPanel() -> impl IntoView {
    let stats = Resource::new(|| (), |_| admin_dashboard_stats());

    view! {
        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4">"Tableau de bord"</h1>

        <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement des statistiques…"</p> }>
            {move || Suspend::new(async move {
                match stats.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(stats) => view! {
                        <div class="flex flex-col gap-6">
                            <div class="grid grid-cols-2 md:grid-cols-5 gap-3">
                                <StatCard label="Utilisateurs" value=stats.user_count/>
                                <StatCard label="Groupes" value=stats.group_count/>
                                <StatCard label="Autorités" value=stats.authority_count/>
                                <StatCard label="Domaines" value=stats.domain_count/>
                                <StatCard label="Fournisseurs OIDC actifs" value=stats.active_oidc_provider_count/>
                            </div>

                            <div>
                                <h2 class="text-base font-bold text-gray-900 dark:text-gray-100 mb-2">"Accès rapide"</h2>
                                <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
                                    <Tile title="Utilisateurs" href="/admin/users".to_string() description="Comptes et permissions"/>
                                    <Tile title="Groupes" href="/admin/groups".to_string() description="Hiérarchie de groupes"/>
                                    <Tile title="Autorités" href="/admin/authorities".to_string() description="Autorités administratives"/>
                                    <Tile title="Domaines" href="/admin/domains".to_string() description="Domaines techniques des actes"/>
                                    <Tile title="Intentions" href="/admin/intentions".to_string() description="Intentions rédactionnelles"/>
                                    <Tile title="Outils de l'agent" href="/admin/agent-tools".to_string() description="Disponibilité par domaine"/>
                                    <Tile title="Fournisseurs OIDC" href="/admin/oidc".to_string() description="Authentification déléguée"/>
                                    <Tile title="Journal d'audit" href="/admin/audit".to_string() description="Actions sensibles tracées"/>
                                </div>
                            </div>

                            <div>
                                <h2 class="text-base font-bold text-gray-900 dark:text-gray-100 mb-2">"Activité récente"</h2>
                                {if stats.recent_audit_events.is_empty() {
                                    view! { <p class="text-sm text-gray-500 dark:text-gray-400">"Aucune activité récente."</p> }.into_any()
                                } else {
                                    view! {
                                        <ul class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm divide-y divide-gray-200 dark:divide-gray-800">
                                            {stats.recent_audit_events.into_iter().map(|entry| view! {
                                                <li class="px-3 py-2 text-sm flex flex-wrap gap-x-2 gap-y-0">
                                                    <span class="text-gray-500 dark:text-gray-400 whitespace-nowrap">{entry.occurred_at}</span>
                                                    <span class="font-bold">{entry.actor}</span>
                                                    <span>{entry.action}</span>
                                                    <span class="text-gray-500 dark:text-gray-400">{entry.resource_type}</span>
                                                </li>
                                            }).collect::<Vec<_>>()}
                                        </ul>
                                    }.into_any()
                                }}
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}
