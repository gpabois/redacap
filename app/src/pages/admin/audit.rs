//! Page `/admin/audit` : consultation du journal d'audit des accès et
//! actions sensibles — voir `Claude.md` § Pages de l'application et
//! contrainte racine « Audit log ».

use dsfr::{Alert, Pagination, Select, SelectOption, Severity, Table};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, admin_context};

const PAGE_SIZE: i64 = 25;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditEntryRow {
    occurred_at: String,
    actor: String,
    actor_ip: Option<String>,
    action: String,
    resource_type: String,
    resource_id: Option<String>,
    details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditPage {
    entries: Vec<AuditEntryRow>,
    total: i64,
}

#[server]
async fn list_audit_log_admin(
    page: i64,
    resource_type: Option<String>,
) -> Result<AuditPage, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let resource_type = resource_type.filter(|value| !value.is_empty());
    let offset = page.max(0) * PAGE_SIZE;
    let entries =
        storage::audit_log::list_audit_events(&pool, resource_type.as_deref(), PAGE_SIZE, offset)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
    let total = storage::audit_log::count_audit_events(&pool, resource_type.as_deref())
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let mut rows = Vec::with_capacity(entries.len());
    for entry in entries {
        let actor = match entry.actor_id {
            Some(entry_actor_id) => storage::user::get_user(&pool, &entry_actor_id)
                .await
                .map(|user| user.display_name)
                .unwrap_or_else(|_| "Utilisateur supprimé".to_string()),
            None => "Système".to_string(),
        };
        rows.push(AuditEntryRow {
            occurred_at: entry.occurred_at.format("%d/%m/%Y %H:%M:%S").to_string(),
            actor,
            actor_ip: entry.actor_ip,
            action: entry.action,
            resource_type: entry.resource_type,
            resource_id: entry.resource_id.map(|id| id.to_string()),
            details: entry
                .details
                .map(|value| serde_json::to_string_pretty(&value).unwrap_or_default()),
        });
    }

    Ok(AuditPage {
        entries: rows,
        total,
    })
}

#[component]
pub fn PageAdminAudit() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::Audit/>
                            <div class="max-w-6xl mx-auto p-6">
                                <AuditPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn AuditPanel() -> impl IntoView {
    let page = RwSignal::new(0usize);
    let (resource_type, set_resource_type) = signal(String::new());

    let audit_page = Resource::new(
        move || (page.get(), resource_type.get()),
        |(page, resource_type)| {
            let resource_type = (!resource_type.is_empty()).then_some(resource_type);
            list_audit_log_admin(page as i64, resource_type)
        },
    );

    let resource_type_options = vec![
        SelectOption::new("", "— Tous les types de ressource —"),
        SelectOption::new("user", "Utilisateur"),
        SelectOption::new("group", "Groupe"),
        SelectOption::new("user_group", "Rattachement à un groupe"),
        SelectOption::new("permission", "Permission"),
        SelectOption::new("authority", "Autorité"),
        SelectOption::new("domain", "Domaine"),
        SelectOption::new("intention", "Intention"),
        SelectOption::new("agent_tool_scope", "Outil de l'agent"),
        SelectOption::new("oidc_provider", "Fournisseur OIDC"),
        SelectOption::new("legal_act", "Acte légal"),
        SelectOption::new("legal_act_intention", "Intention de projet"),
    ];

    view! {
        <h1 class="text-xl font-bold text-gray-900 mb-4">"Journal d'audit"</h1>

        <div class="mb-4 max-w-xs">
            <Select
                label="Type de ressource"
                options=resource_type_options
                value=resource_type
                on_change=move |value| {
                    set_resource_type.set(value);
                    page.set(0);
                }
            />
        </div>

        <Suspense fallback=|| view! { <p class="text-gray-500">"Chargement du journal…"</p> }>
            {move || Suspend::new(async move {
                match audit_page.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(result) => {
                        let total_pages = ((result.total + PAGE_SIZE - 1) / PAGE_SIZE).max(1) as usize;
                        view! {
                            <div class="flex flex-col gap-4">
                                <Table headers=vec!["Horodatage", "Acteur", "IP", "Action", "Ressource", "Détails"]>
                                    {result.entries.into_iter().map(|entry| view! {
                                        <tr class="align-top">
                                            <td class="px-3 py-2 whitespace-nowrap">{entry.occurred_at}</td>
                                            <td class="px-3 py-2">{entry.actor}</td>
                                            <td class="px-3 py-2">{entry.actor_ip.unwrap_or_default()}</td>
                                            <td class="px-3 py-2">{entry.action}</td>
                                            <td class="px-3 py-2">
                                                {entry.resource_type}
                                                {entry.resource_id.map(|id| format!(" ({id})"))}
                                            </td>
                                            <td class="px-3 py-2">
                                                {entry.details.map(|details| view! {
                                                    <pre class="text-xs bg-gray-100 p-2 rounded-sm max-w-md overflow-x-auto">{details}</pre>
                                                })}
                                            </td>
                                        </tr>
                                    }).collect::<Vec<_>>()}
                                </Table>
                                <Pagination current=page total_pages=total_pages/>
                            </div>
                        }.into_any()
                    }
                }
            })}
        </Suspense>
    }
}
