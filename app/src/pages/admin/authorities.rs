//! Page `/admin/authorities` : gestion des autorités administratives
//! référentielles (ex. DREAL, préfecture) pour le compte desquelles un projet
//! d'arrêté est pris — voir `Claude.md` § Pages de l'application.

use dsfr::{Alert, Button, ButtonVariant, Input, Severity, Table};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{AdminAccessDenied, AdminHeader, AdminNav, AdminSection, ConfirmButton, admin_context};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthorityRow {
    id: String,
    nom: String,
    code: String,
    logo_url: Option<String>,
    tutelle: Option<String>,
}

#[server]
async fn list_authorities_admin() -> Result<Vec<AuthorityRow>, ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let authorities = storage::authority::list_authorities(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(authorities
        .into_iter()
        .map(|authority| AuthorityRow {
            id: authority.id.to_string(),
            nom: authority.nom,
            code: authority.code,
            logo_url: authority.logo_url,
            tutelle: authority.tutelle,
        })
        .collect())
}

#[server]
async fn create_authority_admin(
    nom: String,
    code: String,
    logo_url: String,
    tutelle: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let nom = nom.trim().to_string();
    let code = code.trim().to_string();
    if nom.is_empty() || code.is_empty() {
        return Err(ServerFnError::new("nom et code sont obligatoires"));
    }

    let authority = storage::authority::create_authority(
        &pool,
        shared::model::CreateAuthority {
            nom,
            code,
            logo_url: (!logo_url.trim().is_empty()).then(|| logo_url.trim().to_string()),
            tutelle: (!tutelle.trim().is_empty()).then(|| tutelle.trim().to_string()),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "create", "authority", Some(authority.id))
        .await
}

#[server]
async fn update_authority_admin(
    authority_id: String,
    nom: String,
    code: String,
    logo_url: String,
    tutelle: String,
) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let nom = nom.trim().to_string();
    let code = code.trim().to_string();
    if nom.is_empty() || code.is_empty() {
        return Err(ServerFnError::new("nom et code sont obligatoires"));
    }
    let authority_id: shared::id::ID = authority_id
        .parse()
        .map_err(|_| ServerFnError::new("autorité invalide"))?;

    storage::authority::update_authority(
        &pool,
        &authority_id,
        shared::model::AuthorityChangeset {
            nom: Some(nom),
            code: Some(code),
            logo_url: Some((!logo_url.trim().is_empty()).then(|| logo_url.trim().to_string())),
            tutelle: Some((!tutelle.trim().is_empty()).then(|| tutelle.trim().to_string())),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "update", "authority", Some(authority_id))
        .await
}

#[server]
async fn delete_authority_admin(authority_id: String) -> Result<(), ServerFnError> {
    let actor_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    crate::auth::require_admin(&pool, &actor_id).await?;

    let authority_id: shared::id::ID = authority_id
        .parse()
        .map_err(|_| ServerFnError::new("autorité invalide"))?;

    storage::authority::delete_authority(&pool, &authority_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    super::record_admin_audit_event(&pool, actor_id, "delete", "authority", Some(authority_id))
        .await
}

#[component]
pub fn PageAdminAuthorities() -> impl IntoView {
    let context = Resource::new(|| (), |_| admin_context());
    view! {
        <Suspense fallback=|| view! { <p class="p-8 text-gray-500 dark:text-gray-400">"Chargement…"</p> }>
            {move || Suspend::new(async move {
                match context.await {
                    Err(_) => view! { <AdminAccessDenied/> }.into_any(),
                    Ok(access) => view! {
                        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
                            <AdminHeader initial=access.initial.clone()/>
                            <AdminNav active=AdminSection::Authorities/>
                            <div class="max-w-6xl mx-auto p-6">
                                <AuthoritiesPanel/>
                            </div>
                        </div>
                    }.into_any(),
                }
            })}
        </Suspense>
    }
}

#[component]
fn AuthoritiesPanel() -> impl IntoView {
    let version = RwSignal::new(0u32);
    let bump = move || version.update(|v| *v += 1);
    let authorities = Resource::new(move || version.get(), |_| list_authorities_admin());

    let (nom, set_nom) = signal(String::new());
    let (code, set_code) = signal(String::new());
    let (logo_url, set_logo_url) = signal(String::new());
    let (tutelle, set_tutelle) = signal(String::new());
    let (form_error, set_form_error) = signal(Option::<String>::None);
    let editing_id = RwSignal::new(Option::<String>::None);

    let reset_form = move || {
        set_nom.set(String::new());
        set_code.set(String::new());
        set_logo_url.set(String::new());
        set_tutelle.set(String::new());
        set_form_error.set(None);
        editing_id.set(None);
    };

    let create_action = Action::new(move |input: &(String, String, String, String)| {
        let (nom, code, logo_url, tutelle) = input.clone();
        create_authority_admin(nom, code, logo_url, tutelle)
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

    let update_action = Action::new(move |input: &(String, String, String, String, String)| {
        let (id, nom, code, logo_url, tutelle) = input.clone();
        update_authority_admin(id, nom, code, logo_url, tutelle)
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

    let delete_action = Action::new(|id: &String| delete_authority_admin(id.clone()));
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
        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4">"Autorités administratives"</h1>

        <div class="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-sm p-4 mb-6 flex flex-col gap-3">
            <h2 class="text-base font-bold text-gray-900 dark:text-gray-100">
                {move || if editing_id.get().is_some() { "Modifier l'autorité" } else { "Créer une autorité" }}
            </h2>
            {move || form_error.get().map(|message| view! {
                <Alert severity=Severity::Error small=true>{message}</Alert>
            })}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <Input label="Nom" value=nom on_input=move |v| set_nom.set(v)/>
                <Input label="Code" value=code on_input=move |v| set_code.set(v)/>
                <Input label="URL du logo (optionnel)" value=logo_url on_input=move |v| set_logo_url.set(v)/>
                <Input label="Tutelle (optionnel)" value=tutelle on_input=move |v| set_tutelle.set(v)/>
            </div>
            <div class="flex gap-2">
                <Button
                    variant=ButtonVariant::Primary
                    disabled=create_action.pending().get() || update_action.pending().get()
                    on_click=move |_| {
                        if nom.get().trim().is_empty() || code.get().trim().is_empty() {
                            set_form_error.set(Some("Nom et code sont obligatoires.".to_string()));
                            return;
                        }
                        match editing_id.get_untracked() {
                            Some(id) => {
                                update_action.dispatch((id, nom.get(), code.get(), logo_url.get(), tutelle.get()));
                            }
                            None => {
                                create_action.dispatch((nom.get(), code.get(), logo_url.get(), tutelle.get()));
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

        <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement des autorités…"</p> }>
            {move || Suspend::new(async move {
                match authorities.await {
                    Err(error) => view! { <Alert severity=Severity::Error>{error.to_string()}</Alert> }.into_any(),
                    Ok(rows) => view! {
                        <Table headers=vec!["Nom", "Code", "Tutelle", ""]>
                            {rows.into_iter().map(|authority| {
                                let authority_id = authority.id.clone();
                                let edit_snapshot = (
                                    authority.id.clone(),
                                    authority.nom.clone(),
                                    authority.code.clone(),
                                    authority.logo_url.clone().unwrap_or_default(),
                                    authority.tutelle.clone().unwrap_or_default(),
                                );
                                view! {
                                    <tr>
                                        <td class="px-3 py-2">{authority.nom}</td>
                                        <td class="px-3 py-2">{authority.code}</td>
                                        <td class="px-3 py-2">{authority.tutelle.unwrap_or_default()}</td>
                                        <td class="px-3 py-2">
                                            <div class="flex gap-2">
                                                <Button
                                                    variant=ButtonVariant::TertiaryNoOutline
                                                    on_click=move |_| {
                                                        let (id, nom, code, logo_url, tutelle) = edit_snapshot.clone();
                                                        set_nom.set(nom);
                                                        set_code.set(code);
                                                        set_logo_url.set(logo_url);
                                                        set_tutelle.set(tutelle);
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
                                                        delete_action.dispatch(authority_id.clone());
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
