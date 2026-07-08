//! Page `/admin/agent-profiles` : catalogue des agents experts éphémères
//! que le Superviseur peut instancier à la volée (voir
//! `agent::orchestration`, `agent::catalog::AgentCatalog`) — chaque expert
//! (Visas, Motifs, Articles...) n'est qu'une ligne de cette table, jamais
//! une struct Rust dédiée, éditable ici sans redéploiement.

use dsfr::{Alert, Badge, Button, ButtonVariant, Input, Severity, Table, Textarea, Toggle};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, ConfirmButton, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentProfileRow {
    id: String,
    name: String,
    display_name: String,
    system_prompt: String,
    tool_names: Vec<String>,
    max_steps: i32,
    enabled: bool,
}

/// Découpe la zone de texte « un outil par ligne » du formulaire en noms
/// d'outils, en ignorant les lignes vides et les espaces superflus (accepte
/// aussi les virgules, pour coller/couper depuis une liste existante).
fn parse_tool_names(raw: &str) -> Vec<String> {
    raw.split(|c: char| c == '\n' || c == ',')
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

#[server]
async fn list_agent_profiles_admin() -> Result<Vec<AgentProfileRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let profiles = storage::agent_profile::list_agent_profiles(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(profiles
        .into_iter()
        .map(|profile| AgentProfileRow {
            id: profile.id.to_string(),
            name: profile.name,
            display_name: profile.display_name,
            system_prompt: profile.system_prompt,
            tool_names: profile.tool_names,
            max_steps: profile.max_steps,
            enabled: profile.enabled,
        })
        .collect())
}

#[server]
async fn create_agent_profile_admin(
    name: String,
    display_name: String,
    system_prompt: String,
    tool_names: String,
    max_steps: i32,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    let display_name = display_name.trim().to_string();
    if name.is_empty() || display_name.is_empty() {
        return Err(ServerFnError::new(
            "l'identifiant technique et le libellé sont obligatoires",
        ));
    }

    let profile = storage::agent_profile::create_agent_profile(
        &pool,
        shared::model::CreateAgentProfile {
            name,
            display_name,
            system_prompt: system_prompt.trim().to_string(),
            tool_names: parse_tool_names(&tool_names),
            max_steps: max_steps.max(1),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "create", "agent_profile", Some(profile.id)).await
}

#[server]
async fn update_agent_profile_admin(
    profile_id: String,
    name: String,
    display_name: String,
    system_prompt: String,
    tool_names: String,
    max_steps: i32,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let profile_id: shared::id::ID = profile_id
        .parse()
        .map_err(|_| ServerFnError::new("profil invalide"))?;

    storage::agent_profile::update_agent_profile(
        &pool,
        &profile_id,
        shared::model::AgentProfileChangeset {
            name: Some(name.trim().to_string()),
            display_name: Some(display_name.trim().to_string()),
            system_prompt: Some(system_prompt.trim().to_string()),
            tool_names: Some(parse_tool_names(&tool_names)),
            max_steps: Some(max_steps.max(1)),
            enabled: None,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "agent_profile", Some(profile_id)).await
}

#[server]
async fn set_agent_profile_enabled_admin(profile_id: String, enabled: bool) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let profile_id: shared::id::ID = profile_id
        .parse()
        .map_err(|_| ServerFnError::new("profil invalide"))?;

    storage::agent_profile::update_agent_profile(
        &pool,
        &profile_id,
        shared::model::AgentProfileChangeset {
            enabled: Some(enabled),
            ..Default::default()
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "agent_profile", Some(profile_id)).await
}

#[server]
async fn delete_agent_profile_admin(profile_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let profile_id: shared::id::ID = profile_id
        .parse()
        .map_err(|_| ServerFnError::new("profil invalide"))?;

    storage::agent_profile::delete_agent_profile(&pool, &profile_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "delete", "agent_profile", Some(profile_id)).await
}

#[component]
pub fn PageAdminAgentProfiles() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::AgentProfiles/>
                            <div class="max-w-6xl mx-auto p-6">
                                <AgentProfilesPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn AgentProfilesPanel() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);
    let profiles = Resource::new(move || version.get(), |_| list_agent_profiles_admin());

    let (name, set_name) = signal(String::new());
    let (display_name, set_display_name) = signal(String::new());
    let (system_prompt, set_system_prompt) = signal(String::new());
    let (tool_names, set_tool_names) = signal(String::new());
    let (max_steps, set_max_steps) = signal("8".to_string());
    let (form_error, set_form_error) = signal(Option::<String>::None);
    let editing_id = RwSignal::new(Option::<String>::None);

    let reset_form = move || {
        set_name.set(String::new());
        set_display_name.set(String::new());
        set_system_prompt.set(String::new());
        set_tool_names.set(String::new());
        set_max_steps.set("8".to_string());
        set_form_error.set(None);
        editing_id.set(None);
    };

    let create_action = Action::new(move |input: &(String, String, String, String, i32)| {
        let (name, display_name, system_prompt, tool_names, max_steps) = input.clone();
        create_agent_profile_admin(name, display_name, system_prompt, tool_names, max_steps)
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

    let update_action = Action::new(move |input: &(String, String, String, String, String, i32)| {
        let (id, name, display_name, system_prompt, tool_names, max_steps) = input.clone();
        update_agent_profile_admin(id, name, display_name, system_prompt, tool_names, max_steps)
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

    let toggle_enabled = Action::new(|input: &(String, bool)| {
        let (id, enabled) = input.clone();
        set_agent_profile_enabled_admin(id, enabled)
    });
    Effect::new(move |_| {
        if let Some(Ok(())) = toggle_enabled.value().get() {
            bump();
        }
    });

    let delete_action = Action::new(|id: &String| delete_agent_profile_admin(id.clone()));
    Effect::new(move |_| {
        if let Some(Ok(())) = delete_action.value().get() {
            bump();
        }
    });

    view! {
        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4">"Profils d'agents experts"</h1>
        <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
            "Le Superviseur délègue chaque sous-tâche de rédaction à l'un de ces profils via "
            "l'outil « delegate_to_expert ». Un profil désactivé n'est plus proposé au "
            "Superviseur, mais reste conservé."
        </p>

        <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 mb-6 flex flex-col gap-3">
            <h2 class="text-base font-bold text-gray-900 dark:text-gray-100">
                {move || if editing_id.get().is_some() { "Modifier le profil" } else { "Créer un profil" }}
            </h2>
            {move || form_error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Input
                    label="Identifiant technique"
                    hint="ex. visas — valeur du paramètre expert_id transmis au modèle, jamais affichée"
                    value=name
                    on_input=move |v| set_name.set(v)
                />
                <Input label="Libellé affiché" hint="ex. Expert Visas" value=display_name on_input=move |v| set_display_name.set(v)/>
                <Input
                    label="Nombre maximal de tours"
                    r#type="number"
                    value=max_steps
                    on_input=move |v| set_max_steps.set(v)
                />
            </div>
            <Textarea
                label="Prompt système"
                hint="Rôle et consignes de cet expert : il ne voit que ce prompt et la sous-tâche confiée par le Superviseur, jamais le reste de la conversation."
                value=system_prompt
                on_input=move |v| set_system_prompt.set(v)
            />
            <Textarea
                label="Outils autorisés"
                hint="Un nom d'outil technique par ligne (ex. read_structure, fill_section, insert_node, legifrance_search, ask_user...)."
                value=tool_names
                on_input=move |v| set_tool_names.set(v)
            />
            <div class="flex gap-2">
                <Button
                    variant=ButtonVariant::Primary
                    disabled=create_action.pending().get() || update_action.pending().get()
                    on_click=move |_| {
                        if name.get().trim().is_empty() || display_name.get().trim().is_empty() {
                            set_form_error.set(Some("L'identifiant technique et le libellé sont obligatoires.".to_string()));
                            return;
                        }
                        let max_steps = max_steps.get().trim().parse::<i32>().unwrap_or(8);
                        match editing_id.get_untracked() {
                            Some(id) => {
                                update_action.dispatch((
                                    id,
                                    name.get(),
                                    display_name.get(),
                                    system_prompt.get(),
                                    tool_names.get(),
                                    max_steps,
                                ));
                            }
                            None => {
                                create_action.dispatch((
                                    name.get(),
                                    display_name.get(),
                                    system_prompt.get(),
                                    tool_names.get(),
                                    max_steps,
                                ));
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

        <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement des profils…"</p> }>
            {move || Suspend::new(async move {
                match profiles.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => view! {
                        <Table headers=vec!["Identifiant", "Libellé", "Outils", "Actif", ""]>
                            {rows.into_iter().map(|row| {
                                let row_id = row.id.clone();
                                let row_id_for_toggle = row.id.clone();
                                let edit_snapshot = (
                                    row.id.clone(),
                                    row.name.clone(),
                                    row.display_name.clone(),
                                    row.system_prompt.clone(),
                                    row.tool_names.join("\n"),
                                    row.max_steps,
                                );
                                let enabled = row.enabled;
                                let tool_count = row.tool_names.len();
                                view! {
                                    <tr>
                                        <td class="px-3 py-2 font-mono text-xs">{row.name}</td>
                                        <td class="px-3 py-2">{row.display_name}</td>
                                        <td class="px-3 py-2">
                                            <Badge severity=Severity::Info small=true>
                                                {format!("{tool_count} outil(s)")}
                                            </Badge>
                                        </td>
                                        <td class="px-3 py-2">
                                            <Toggle
                                                label=""
                                                checked=enabled
                                                on_toggle=move |enabled| {
                                                    toggle_enabled.dispatch((row_id_for_toggle.clone(), enabled));
                                                }
                                            />
                                        </td>
                                        <td class="px-3 py-2">
                                            <div class="flex gap-2">
                                                <Button
                                                    variant=ButtonVariant::TertiaryNoOutline
                                                    on_click=move |_| {
                                                        let (id, name, display_name, system_prompt, tool_names, max_steps) = edit_snapshot.clone();
                                                        set_name.set(name);
                                                        set_display_name.set(display_name);
                                                        set_system_prompt.set(system_prompt);
                                                        set_tool_names.set(tool_names);
                                                        set_max_steps.set(max_steps.to_string());
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
                                                        delete_action.dispatch(row_id.clone());
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
