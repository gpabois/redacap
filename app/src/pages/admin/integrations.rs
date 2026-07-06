//! Page `/admin/integrations` : configuration chiffrée des accès aux API
//! externes GéoRisques et Légifrance — voir `Claude.md` § Pages de
//! l'application et `shared::model::{GeorisquesCredentials, LegifranceCredentials}`.
//! Ces secrets partagent la même clé de chiffrement (`SECRET_ENCRYPTION_KEY`)
//! que les modèles IA (`/admin/ai-models`) et les fournisseurs OIDC
//! (`/admin/oidc`).

use dsfr::{Alert, Button, ButtonVariant, Input, Severity};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, ConfirmButton, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IntegrationsStatus {
    georisques_configured: bool,
    legifrance_client_id: Option<String>,
    legifrance_configured: bool,
}

#[server]
async fn get_integrations_admin() -> Result<IntegrationsStatus, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let georisques = storage::external_credentials::get_georisques_credentials(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let legifrance = storage::external_credentials::get_legifrance_credentials(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(IntegrationsStatus {
        georisques_configured: georisques.is_some_and(|c| c.api_key_encrypted.is_some()),
        legifrance_client_id: legifrance.as_ref().and_then(|c| c.client_id.clone()),
        legifrance_configured: legifrance
            .is_some_and(|c| c.client_id.is_some() && c.client_secret_encrypted.is_some()),
    })
}

#[server]
async fn set_georisques_api_key_admin(api_key: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let encryption_key = expect_context::<Option<Vec<u8>>>().ok_or_else(|| {
        ServerFnError::new("chiffrement indisponible (SECRET_ENCRYPTION_KEY absente)")
    })?;
    let api_key_encrypted = shared::crypto::encrypt(&encryption_key, &api_key)
        .map_err(|_| ServerFnError::new("échec du chiffrement de la clé API"))?;

    storage::external_credentials::set_georisques_credentials(
        &pool,
        shared::model::SetGeorisquesCredentials {
            api_key_encrypted: Some(api_key_encrypted),
            updated_by: Some(actor_id.clone()),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "georisques_credentials", None).await
}

#[server]
async fn clear_georisques_api_key_admin() -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    storage::external_credentials::set_georisques_credentials(
        &pool,
        shared::model::SetGeorisquesCredentials {
            api_key_encrypted: None,
            updated_by: Some(actor_id.clone()),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "clear", "georisques_credentials", None).await
}

#[server]
async fn set_legifrance_credentials_admin(
    client_id: String,
    client_secret: Option<String>,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let client_id = client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(ServerFnError::new("le client_id est obligatoire"));
    }

    let client_secret_encrypted = match client_secret.filter(|secret| !secret.is_empty()) {
        Some(secret) => {
            let encryption_key = expect_context::<Option<Vec<u8>>>().ok_or_else(|| {
                ServerFnError::new("chiffrement indisponible (SECRET_ENCRYPTION_KEY absente)")
            })?;
            Some(
                shared::crypto::encrypt(&encryption_key, &secret)
                    .map_err(|_| ServerFnError::new("échec du chiffrement du secret"))?,
            )
        }
        None => {
            let current = storage::external_credentials::get_legifrance_credentials(&pool)
                .await
                .map_err(|error| ServerFnError::new(error.to_string()))?;
            let current_secret = current.and_then(|c| c.client_secret_encrypted);
            if current_secret.is_none() {
                return Err(ServerFnError::new("le client_secret est obligatoire"));
            }
            current_secret
        }
    };

    storage::external_credentials::set_legifrance_credentials(
        &pool,
        shared::model::SetLegifranceCredentials {
            client_id: Some(client_id),
            client_secret_encrypted,
            updated_by: Some(actor_id.clone()),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "legifrance_credentials", None).await
}

#[server]
async fn clear_legifrance_credentials_admin() -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    storage::external_credentials::set_legifrance_credentials(
        &pool,
        shared::model::SetLegifranceCredentials {
            client_id: None,
            client_secret_encrypted: None,
            updated_by: Some(actor_id.clone()),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "clear", "legifrance_credentials", None).await
}

#[component]
pub fn PageAdminIntegrations() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::Integrations/>
                            <div class="max-w-6xl mx-auto p-6 flex flex-col gap-6">
                                <IntegrationsPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn IntegrationsPanel() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);
    let status = Resource::new(move || version.get(), |_| get_integrations_admin());
    let (error, set_error) = signal(Option::<String>::None);

    let (georisques_key, set_georisques_key) = signal(String::new());
    let save_georisques = Action::new(|key: &String| set_georisques_api_key_admin(key.clone()));
    Effect::new(move |_| {
        if let Some(result) = save_georisques.value().get() {
            match result {
                Ok(()) => {
                    set_georisques_key.set(String::new());
                    bump();
                }
                Err(err) => set_error.set(Some(err.to_string())),
            }
        }
    });
    let clear_georisques = Action::new(|_: &()| clear_georisques_api_key_admin());
    Effect::new(move |_| {
        if let Some(result) = clear_georisques.value().get() {
            match result {
                Ok(()) => bump(),
                Err(err) => set_error.set(Some(err.to_string())),
            }
        }
    });

    let (legifrance_client_id, set_legifrance_client_id) = signal(String::new());
    let (legifrance_client_secret, set_legifrance_client_secret) = signal(String::new());
    let save_legifrance = Action::new(move |input: &(String, String)| {
        let (client_id, client_secret) = input.clone();
        set_legifrance_credentials_admin(
            client_id,
            (!client_secret.is_empty()).then_some(client_secret),
        )
    });
    Effect::new(move |_| {
        if let Some(result) = save_legifrance.value().get() {
            match result {
                Ok(()) => {
                    set_legifrance_client_secret.set(String::new());
                    bump();
                }
                Err(err) => set_error.set(Some(err.to_string())),
            }
        }
    });
    let clear_legifrance = Action::new(|_: &()| clear_legifrance_credentials_admin());
    Effect::new(move |_| {
        if let Some(result) = clear_legifrance.value().get() {
            match result {
                Ok(()) => {
                    set_legifrance_client_id.set(String::new());
                    set_legifrance_client_secret.set(String::new());
                    bump();
                }
                Err(err) => set_error.set(Some(err.to_string())),
            }
        }
    });

    view! {
        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100">"Intégrations externes"</h1>

        {move || error.get().map(|message| view! {
            <Alert severity=Severity::Error small=true>{message}</Alert>
        })}

        <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match status.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(status) => view! {
                        <div class="flex flex-col gap-6">
                            <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 flex flex-col gap-3">
                                <div class="flex items-center justify-between">
                                    <h2 class="text-base font-bold text-gray-900 dark:text-gray-100">"GéoRisques"</h2>
                                    {if status.georisques_configured {
                                        view! { <span class="text-sm text-success font-bold">"Clé configurée"</span> }.into_any()
                                    } else {
                                        view! { <span class="text-sm text-gray-500 dark:text-gray-400">"Non configurée (API v1 accessible sans jeton, quota réduit)"</span> }.into_any()
                                    }}
                                </div>
                                <Input
                                    label="Clé API"
                                    r#type="password"
                                    hint="Laisser vide et enregistrer n'a aucun effet ; utilisez « Supprimer » pour effacer la clé actuelle."
                                    value=georisques_key
                                    on_input=move |v| set_georisques_key.set(v)
                                />
                                <div class="flex gap-2">
                                    <Button
                                        variant=ButtonVariant::Primary
                                        disabled=save_georisques.pending().get() || georisques_key.get().is_empty()
                                        on_click=move |_| { save_georisques.dispatch(georisques_key.get()); }
                                    >
                                        "Enregistrer"
                                    </Button>
                                    {status.georisques_configured.then(|| view! {
                                        <ConfirmButton
                                            label="Supprimer la clé"
                                            confirm_label="Confirmer ?"
                                            disabled=clear_georisques.pending().get()
                                            on_confirm=Callback::new(move |_| { clear_georisques.dispatch(()); })
                                        />
                                    })}
                                </div>
                            </div>

                            <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 flex flex-col gap-3">
                                <div class="flex items-center justify-between">
                                    <h2 class="text-base font-bold text-gray-900 dark:text-gray-100">"Légifrance (PISTE)"</h2>
                                    {if status.legifrance_configured {
                                        view! { <span class="text-sm text-success font-bold">"Configuré"</span> }.into_any()
                                    } else {
                                        view! { <span class="text-sm text-gray-500 dark:text-gray-400">"Non configuré : outils legifrance_search/legifrance_fetch indisponibles"</span> }.into_any()
                                    }}
                                </div>
                                <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                    <Input
                                        label="Client ID"
                                        value=legifrance_client_id
                                        on_input=move |v| set_legifrance_client_id.set(v)
                                    />
                                    <Input
                                        label="Client secret"
                                        r#type="password"
                                        hint="Laisser vide pour conserver le secret actuel."
                                        value=legifrance_client_secret
                                        on_input=move |v| set_legifrance_client_secret.set(v)
                                    />
                                </div>
                                <div class="flex gap-2">
                                    <Button
                                        variant=ButtonVariant::Primary
                                        disabled=save_legifrance.pending().get() || legifrance_client_id.get().trim().is_empty()
                                        on_click=move |_| {
                                            save_legifrance.dispatch((legifrance_client_id.get(), legifrance_client_secret.get()));
                                        }
                                    >
                                        "Enregistrer"
                                    </Button>
                                    {status.legifrance_configured.then(|| view! {
                                        <ConfirmButton
                                            label="Supprimer la configuration"
                                            confirm_label="Confirmer ?"
                                            disabled=clear_legifrance.pending().get()
                                            on_confirm=Callback::new(move |_| { clear_legifrance.dispatch(()); })
                                        />
                                    })}
                                </div>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}
