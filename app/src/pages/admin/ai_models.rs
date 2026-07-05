//! Page `/admin/ai-models` : modèles de langage compatibles OpenAI
//! configurables comme moteur de l'agent IA « Marie » — voir `Claude.md` §
//! Pages de l'application et `shared::model::AiModel`.

use dsfr::{Alert, Badge, Button, ButtonVariant, Input, Severity, Table, Textarea};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, ConfirmButton, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiModelRow {
    id: String,
    name: String,
    base_url: String,
    model: String,
    system_prompt: String,
    active: bool,
}

#[server]
async fn list_ai_models_admin() -> Result<Vec<AiModelRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let models = storage::ai_model::list_ai_models(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(models
        .into_iter()
        .map(|model| AiModelRow {
            id: model.id.to_string(),
            name: model.name,
            base_url: model.base_url,
            model: model.model,
            system_prompt: model.system_prompt,
            active: model.active,
        })
        .collect())
}

#[server]
async fn create_ai_model_admin(
    name: String,
    base_url: String,
    model: String,
    api_key: String,
    system_prompt: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    let base_url = base_url.trim().to_string();
    let model = model.trim().to_string();
    if name.is_empty() || base_url.is_empty() || model.is_empty() || api_key.is_empty() {
        return Err(ServerFnError::new(
            "nom, URL de base, modèle et clé API sont obligatoires",
        ));
    }

    let encryption_key = expect_context::<Option<[u8; 32]>>().ok_or_else(|| {
        ServerFnError::new("chiffrement indisponible (SECRET_ENCRYPTION_KEY absente)")
    })?;
    let api_key_encrypted = shared::crypto::encrypt(&encryption_key, &api_key)
        .map_err(|_| ServerFnError::new("échec du chiffrement de la clé API"))?;

    let ai_model = storage::ai_model::create_ai_model(
        &pool,
        shared::model::CreateAiModel {
            name,
            base_url,
            model,
            api_key_encrypted,
            system_prompt: system_prompt.trim().to_string(),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "create", "ai_model", Some(ai_model.id)).await
}

#[server]
async fn update_ai_model_admin(
    model_id: String,
    name: String,
    base_url: String,
    model: String,
    new_api_key: Option<String>,
    system_prompt: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let model_id: shared::id::ID = model_id
        .parse()
        .map_err(|_| ServerFnError::new("modèle invalide"))?;

    let api_key_encrypted = match new_api_key.filter(|key| !key.is_empty()) {
        Some(key) => {
            let encryption_key = expect_context::<Option<[u8; 32]>>().ok_or_else(|| {
                ServerFnError::new("chiffrement indisponible (SECRET_ENCRYPTION_KEY absente)")
            })?;
            Some(
                shared::crypto::encrypt(&encryption_key, &key)
                    .map_err(|_| ServerFnError::new("échec du chiffrement de la clé API"))?,
            )
        }
        None => None,
    };

    storage::ai_model::update_ai_model(
        &pool,
        &model_id,
        shared::model::AiModelChangeset {
            name: Some(name.trim().to_string()),
            base_url: Some(base_url.trim().to_string()),
            model: Some(model.trim().to_string()),
            api_key_encrypted,
            system_prompt: Some(system_prompt.trim().to_string()),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "ai_model", Some(model_id)).await
}

#[server]
async fn set_active_ai_model_admin(model_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let model_id: shared::id::ID = model_id
        .parse()
        .map_err(|_| ServerFnError::new("modèle invalide"))?;

    storage::ai_model::set_active_ai_model(&pool, &model_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "activate", "ai_model", Some(model_id)).await
}

#[server]
async fn delete_ai_model_admin(model_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let model_id: shared::id::ID = model_id
        .parse()
        .map_err(|_| ServerFnError::new("modèle invalide"))?;

    storage::ai_model::delete_ai_model(&pool, &model_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "delete", "ai_model", Some(model_id)).await
}

#[component]
pub fn PageAdminAiModels() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::AiModels/>
                            <div class="max-w-6xl mx-auto p-6">
                                <AiModelsPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn AiModelsPanel() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);
    let models = Resource::new(move || version.get(), |_| list_ai_models_admin());

    let (name, set_name) = signal(String::new());
    let (base_url, set_base_url) = signal(String::new());
    let (model, set_model) = signal(String::new());
    let (api_key, set_api_key) = signal(String::new());
    let (system_prompt, set_system_prompt) = signal(String::new());
    let (form_error, set_form_error) = signal(Option::<String>::None);
    let editing_id = RwSignal::new(Option::<String>::None);

    let reset_form = move || {
        set_name.set(String::new());
        set_base_url.set(String::new());
        set_model.set(String::new());
        set_api_key.set(String::new());
        set_system_prompt.set(String::new());
        set_form_error.set(None);
        editing_id.set(None);
    };

    let create_action = Action::new(move |input: &(String, String, String, String, String)| {
        let (name, base_url, model, api_key, system_prompt) = input.clone();
        create_ai_model_admin(name, base_url, model, api_key, system_prompt)
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

    let update_action = Action::new(
        move |input: &(String, String, String, String, Option<String>, String)| {
            let (id, name, base_url, model, api_key, system_prompt) = input.clone();
            update_ai_model_admin(id, name, base_url, model, api_key, system_prompt)
        },
    );
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

    let activate_action = Action::new(|id: &String| set_active_ai_model_admin(id.clone()));
    Effect::new(move |_| {
        if let Some(Ok(())) = activate_action.value().get() {
            bump();
        }
    });

    let delete_action = Action::new(|id: &String| delete_ai_model_admin(id.clone()));
    Effect::new(move |_| {
        if let Some(Ok(())) = delete_action.value().get() {
            bump();
        }
    });

    view! {
        <h1 class="text-xl font-bold text-gray-900 mb-4">"Modèles IA"</h1>
        <p class="text-sm text-gray-600 mb-4">
            "Points de terminaison compatibles avec l'API de complétion de chat OpenAI. "
            "Le modèle marqué « Actif » est utilisé comme moteur de l'agent IA « Marie »."
        </p>

        <div class="bg-white border border-gray-200 rounded-sm p-4 mb-6 flex flex-col gap-3">
            <h2 class="text-base font-bold text-gray-900">
                {move || if editing_id.get().is_some() { "Modifier le modèle" } else { "Enregistrer un modèle" }}
            </h2>
            {move || form_error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Input label="Nom" value=name on_input=move |v| set_name.set(v)/>
                <Input label="Identifiant du modèle" value=model on_input=move |v| set_model.set(v) hint="ex. gpt-4o-mini"/>
                <Input label="URL de base" value=base_url on_input=move |v| set_base_url.set(v) hint="ex. https://api.openai.com/v1"/>
                <Input
                    label="Clé API"
                    r#type="password"
                    hint="Laisser vide pour conserver la clé actuelle lors d'une modification."
                    value=api_key
                    on_input=move |v| set_api_key.set(v)
                />
            </div>
            <Textarea
                label="Prompt système dédié"
                hint="Ajouté en entête des contextes de domaine et d'intentions."
                value=system_prompt
                on_input=move |v| set_system_prompt.set(v)
            />
            <div class="flex gap-2">
                <Button
                    variant=ButtonVariant::Primary
                    disabled=create_action.pending().get() || update_action.pending().get()
                    on_click=move |_| {
                        if name.get().trim().is_empty() || base_url.get().trim().is_empty()
                            || model.get().trim().is_empty()
                            || (editing_id.get_untracked().is_none() && api_key.get().is_empty()) {
                            set_form_error.set(Some("Nom, URL de base, modèle et clé API sont obligatoires.".to_string()));
                            return;
                        }
                        match editing_id.get_untracked() {
                            Some(id) => {
                                let key = api_key.get();
                                update_action.dispatch((
                                    id,
                                    name.get(),
                                    base_url.get(),
                                    model.get(),
                                    (!key.is_empty()).then_some(key),
                                    system_prompt.get(),
                                ));
                            }
                            None => {
                                create_action.dispatch((name.get(), base_url.get(), model.get(), api_key.get(), system_prompt.get()));
                            }
                        }
                    }
                >
                    {move || if editing_id.get().is_some() { "Enregistrer les modifications" } else { "Enregistrer" }}
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

        <Suspense fallback=|| view! { <p class="text-gray-500">"Chargement des modèles…"</p> }>
            {move || Suspend::new(async move {
                match models.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => view! {
                        <Table headers=vec!["Nom", "Modèle", "URL de base", "Statut", ""]>
                            {rows.into_iter().map(|row| {
                                let row_id = row.id.clone();
                                let row_id_for_activate = row.id.clone();
                                let edit_snapshot = (
                                    row.id.clone(),
                                    row.name.clone(),
                                    row.base_url.clone(),
                                    row.model.clone(),
                                    row.system_prompt.clone(),
                                );
                                let active = row.active;
                                view! {
                                    <tr>
                                        <td class="px-3 py-2">{row.name}</td>
                                        <td class="px-3 py-2 break-all">{row.model}</td>
                                        <td class="px-3 py-2 break-all">{row.base_url}</td>
                                        <td class="px-3 py-2">
                                            {if active {
                                                view! { <Badge severity=Severity::Success small=true>"Actif"</Badge> }.into_any()
                                            } else {
                                                view! {
                                                    <Button
                                                        variant=ButtonVariant::TertiaryNoOutline
                                                        disabled=activate_action.pending().get()
                                                        on_click=move |_| { activate_action.dispatch(row_id_for_activate.clone()); }
                                                    >
                                                        "Définir comme moteur"
                                                    </Button>
                                                }.into_any()
                                            }}
                                        </td>
                                        <td class="px-3 py-2">
                                            <div class="flex gap-2">
                                                <Button
                                                    variant=ButtonVariant::TertiaryNoOutline
                                                    on_click=move |_| {
                                                        let (id, name, base_url, model, system_prompt) = edit_snapshot.clone();
                                                        set_name.set(name);
                                                        set_base_url.set(base_url);
                                                        set_model.set(model);
                                                        set_api_key.set(String::new());
                                                        set_system_prompt.set(system_prompt);
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
