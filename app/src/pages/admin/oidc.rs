//! Page `/admin/oidc` : configuration des fournisseurs OpenID Connect
//! autorisés — voir `Claude.md` § Pages de l'application et § Authentification.

use dsfr::{Alert, Button, ButtonVariant, Input, Severity, Table, Tag, Toggle};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, ConfirmButton, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OidcProviderRow {
    id: String,
    name: String,
    issuer_url: String,
    client_id: String,
    scopes: Vec<String>,
    active: bool,
    callback_url: Option<String>,
}

#[cfg(feature = "ssr")]
fn parse_scopes(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|scope| scope.trim().to_string())
        .filter(|scope| !scope.is_empty())
        .collect()
}

#[server]
async fn list_oidc_providers_admin() -> Result<Vec<OidcProviderRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let public_base_url = use_context::<Option<String>>().flatten();
    let providers = storage::oidc_provider::list_active_oidc_providers(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(providers
        .into_iter()
        .map(|provider| OidcProviderRow {
            callback_url: public_base_url
                .as_ref()
                .map(|base| format!("{base}/oidc/{}/callback", provider.id)),
            id: provider.id.to_string(),
            name: provider.name,
            issuer_url: provider.issuer_url,
            client_id: provider.client_id,
            scopes: provider.scopes,
            active: provider.active,
        })
        .collect())
}

#[server]
async fn create_oidc_provider_admin(
    name: String,
    issuer_url: String,
    client_id: String,
    client_secret: String,
    scopes: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let name = name.trim().to_string();
    let issuer_url = issuer_url.trim().to_string();
    let client_id = client_id.trim().to_string();
    if name.is_empty() || issuer_url.is_empty() || client_id.is_empty() || client_secret.is_empty()
    {
        return Err(ServerFnError::new(
            "nom, issuer, client_id et secret sont obligatoires",
        ));
    }

    let encryption_key = expect_context::<Option<[u8; 32]>>().ok_or_else(|| {
        ServerFnError::new("chiffrement indisponible (SECRET_ENCRYPTION_KEY absente)")
    })?;
    let client_secret_encrypted = shared::crypto::encrypt(&encryption_key, &client_secret)
        .map_err(|_| ServerFnError::new("échec du chiffrement du secret"))?;

    let provider = storage::oidc_provider::create_oidc_provider(
        &pool,
        shared::model::CreateOidcProvider {
            name,
            issuer_url,
            client_id,
            client_secret_encrypted,
            scopes: parse_scopes(&scopes),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(
        &pool,
        actor_id,
        "create",
        "oidc_provider",
        Some(provider.id),
    )
    .await
}

#[server]
async fn update_oidc_provider_admin(
    provider_id: String,
    name: String,
    issuer_url: String,
    client_id: String,
    new_secret: Option<String>,
    scopes: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let provider_id: shared::id::ID = provider_id
        .parse()
        .map_err(|_| ServerFnError::new("fournisseur invalide"))?;

    let client_secret_encrypted = match new_secret.filter(|secret| !secret.is_empty()) {
        Some(secret) => {
            let encryption_key = expect_context::<Option<[u8; 32]>>().ok_or_else(|| {
                ServerFnError::new("chiffrement indisponible (SECRET_ENCRYPTION_KEY absente)")
            })?;
            Some(
                shared::crypto::encrypt(&encryption_key, &secret)
                    .map_err(|_| ServerFnError::new("échec du chiffrement du secret"))?,
            )
        }
        None => None,
    };

    storage::oidc_provider::update_oidc_provider(
        &pool,
        &provider_id,
        shared::model::OidcProviderChangeset {
            name: Some(name.trim().to_string()),
            issuer_url: Some(issuer_url.trim().to_string()),
            client_id: Some(client_id.trim().to_string()),
            client_secret_encrypted,
            scopes: Some(parse_scopes(&scopes)),
            active: None,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(
        &pool,
        actor_id,
        "update",
        "oidc_provider",
        Some(provider_id),
    )
    .await
}

#[server]
async fn set_oidc_provider_active_admin(
    provider_id: String,
    active: bool,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let provider_id: shared::id::ID = provider_id
        .parse()
        .map_err(|_| ServerFnError::new("fournisseur invalide"))?;

    storage::oidc_provider::update_oidc_provider(
        &pool,
        &provider_id,
        shared::model::OidcProviderChangeset {
            active: Some(active),
            ..Default::default()
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    let action = if active { "activate" } else { "deactivate" };
    super::record_admin_audit_event(&pool, actor_id, action, "oidc_provider", Some(provider_id))
        .await
}

#[server]
async fn delete_oidc_provider_admin(provider_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let provider_id: shared::id::ID = provider_id
        .parse()
        .map_err(|_| ServerFnError::new("fournisseur invalide"))?;

    storage::oidc_provider::delete_oidc_provider(&pool, &provider_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(
        &pool,
        actor_id,
        "delete",
        "oidc_provider",
        Some(provider_id),
    )
    .await
}

#[component]
pub fn PageAdminOidc() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::Oidc/>
                            <div class="max-w-6xl mx-auto p-6">
                                <OidcPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn OidcPanel() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);
    let providers = Resource::new(move || version.get(), |_| list_oidc_providers_admin());

    let (name, set_name) = signal(String::new());
    let (issuer_url, set_issuer_url) = signal(String::new());
    let (client_id, set_client_id) = signal(String::new());
    let (client_secret, set_client_secret) = signal(String::new());
    let (scopes, set_scopes) = signal(String::new());
    let (form_error, set_form_error) = signal(Option::<String>::None);
    let editing_id = RwSignal::new(Option::<String>::None);

    let reset_form = move || {
        set_name.set(String::new());
        set_issuer_url.set(String::new());
        set_client_id.set(String::new());
        set_client_secret.set(String::new());
        set_scopes.set(String::new());
        set_form_error.set(None);
        editing_id.set(None);
    };

    let create_action = Action::new(move |input: &(String, String, String, String, String)| {
        let (name, issuer_url, client_id, client_secret, scopes) = input.clone();
        create_oidc_provider_admin(name, issuer_url, client_id, client_secret, scopes)
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
            let (id, name, issuer_url, client_id, secret, scopes) = input.clone();
            update_oidc_provider_admin(id, name, issuer_url, client_id, secret, scopes)
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

    let toggle_action = Action::new(|input: &(String, bool)| {
        let (id, active) = input.clone();
        set_oidc_provider_active_admin(id, active)
    });
    Effect::new(move |_| {
        if let Some(Ok(())) = toggle_action.value().get() {
            bump();
        }
    });

    let delete_action = Action::new(|id: &String| delete_oidc_provider_admin(id.clone()));
    Effect::new(move |_| {
        if let Some(Ok(())) = delete_action.value().get() {
            bump();
        }
    });

    view! {
        <h1 class="text-xl font-bold text-gray-900 mb-4">"Fournisseurs OpenID Connect"</h1>

        <div class="bg-white border border-gray-200 rounded-sm p-4 mb-6 flex flex-col gap-3">
            <h2 class="text-base font-bold text-gray-900">
                {move || if editing_id.get().is_some() { "Modifier le fournisseur" } else { "Enregistrer un fournisseur" }}
            </h2>
            {move || form_error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Input label="Nom" value=name on_input=move |v| set_name.set(v)/>
                <Input label="Issuer URL" value=issuer_url on_input=move |v| set_issuer_url.set(v)/>
                <Input label="Client ID" value=client_id on_input=move |v| set_client_id.set(v)/>
                <Input
                    label="Client secret"
                    r#type="password"
                    hint="Laisser vide pour conserver le secret actuel lors d'une modification."
                    value=client_secret
                    on_input=move |v| set_client_secret.set(v)
                />
                <Input label="Scopes (séparés par des virgules)" value=scopes on_input=move |v| set_scopes.set(v)/>
            </div>
            <div class="flex gap-2">
                <Button
                    variant=ButtonVariant::Primary
                    disabled=create_action.pending().get() || update_action.pending().get()
                    on_click=move |_| {
                        if name.get().trim().is_empty() || issuer_url.get().trim().is_empty()
                            || client_id.get().trim().is_empty()
                            || (editing_id.get_untracked().is_none() && client_secret.get().is_empty()) {
                            set_form_error.set(Some("Nom, issuer, client_id et secret sont obligatoires.".to_string()));
                            return;
                        }
                        match editing_id.get_untracked() {
                            Some(id) => {
                                let secret = client_secret.get();
                                update_action.dispatch((
                                    id,
                                    name.get(),
                                    issuer_url.get(),
                                    client_id.get(),
                                    (!secret.is_empty()).then_some(secret),
                                    scopes.get(),
                                ));
                            }
                            None => {
                                create_action.dispatch((name.get(), issuer_url.get(), client_id.get(), client_secret.get(), scopes.get()));
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

        <Suspense fallback=|| view! { <p class="text-gray-500">"Chargement des fournisseurs…"</p> }>
            {move || Suspend::new(async move {
                match providers.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => view! {
                        <Table headers=vec!["Nom", "Issuer", "Client ID", "Scopes", "URL de callback", "Actif", ""]>
                            {rows.into_iter().map(|provider| {
                                let provider_id = provider.id.clone();
                                let provider_id_for_toggle = provider.id.clone();
                                let edit_snapshot = (
                                    provider.id.clone(),
                                    provider.name.clone(),
                                    provider.issuer_url.clone(),
                                    provider.client_id.clone(),
                                    provider.scopes.join(", "),
                                );
                                let active = RwSignal::new(provider.active);
                                view! {
                                    <tr>
                                        <td class="px-3 py-2">{provider.name}</td>
                                        <td class="px-3 py-2 break-all">{provider.issuer_url}</td>
                                        <td class="px-3 py-2 break-all">{provider.client_id}</td>
                                        <td class="px-3 py-2">
                                            <div class="flex flex-wrap gap-1">
                                                {provider.scopes.into_iter().map(|scope| view! {
                                                    <Tag on_click=|_| {}>{scope}</Tag>
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        </td>
                                        <td class="px-3 py-2 break-all text-xs text-gray-500">
                                            {provider.callback_url.unwrap_or_else(|| "PUBLIC_BASE_URL non configurée".to_string())}
                                        </td>
                                        <td class="px-3 py-2">
                                            <Toggle
                                                label=""
                                                checked=active
                                                on_toggle=move |checked| {
                                                    active.set(checked);
                                                    toggle_action.dispatch((provider_id_for_toggle.clone(), checked));
                                                }
                                            />
                                        </td>
                                        <td class="px-3 py-2">
                                            <div class="flex gap-2">
                                                <Button
                                                    variant=ButtonVariant::TertiaryNoOutline
                                                    on_click=move |_| {
                                                        let (id, name, issuer_url, client_id, scopes) = edit_snapshot.clone();
                                                        set_name.set(name);
                                                        set_issuer_url.set(issuer_url);
                                                        set_client_id.set(client_id);
                                                        set_client_secret.set(String::new());
                                                        set_scopes.set(scopes);
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
                                                        delete_action.dispatch(provider_id.clone());
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
