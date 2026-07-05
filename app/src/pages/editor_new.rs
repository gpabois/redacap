//! Page `/editor/new` : création d'un projet d'acte légal (autorité, domaine,
//! titre) — voir `Claude.md` § Pages de l'application. Le domaine est choisi
//! une fois pour toutes ici : il ne peut plus être modifié après création
//! (voir `Claude.md` § « Ajoute aux projets... »). Après création réussie,
//! redirige vers `/editor/{id}` avec l'identifiant émis par le serveur.

use dsfr::{Alert, Button, ButtonVariant, Input, Select, SelectOption};
use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;
use serde::{Deserialize, Serialize};

/// Autorité administrative telle qu'exposée à cette page.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthorityOption {
    id: String,
    nom: String,
}

/// Domaine technique tel qu'exposé à cette page (filtré sur les domaines
/// accessibles à l'utilisateur courant).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DomainOption {
    id: String,
    name: String,
}

#[server]
async fn list_authorities() -> Result<Vec<AuthorityOption>, ServerFnError> {
    let pool = expect_context::<storage::Pool>();
    let authorities = storage::authority::list_authorities(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(authorities
        .into_iter()
        .map(|authority| AuthorityOption {
            id: authority.id.to_string(),
            nom: authority.nom,
        })
        .collect())
}

/// Liste les domaines pour lesquels l'utilisateur courant dispose d'un droit
/// (permission `resource_type == "domain"`, action `"use"` — voir
/// `crate::auth::accessible_domain_ids`) : seuls ceux-ci sont proposés à la
/// création d'un projet.
#[server]
async fn list_domains_for_current_user() -> Result<Vec<DomainOption>, ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let accessible_ids = crate::auth::accessible_domain_ids(&pool, &user_id).await?;

    let domains = storage::domain::list_domains(&pool)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(domains
        .into_iter()
        .filter(|domain| accessible_ids.contains(&domain.id))
        .map(|domain| DomainOption {
            id: domain.id.to_string(),
            name: domain.name,
        })
        .collect())
}

/// Crée un projet d'acte légal pour le compte de l'utilisateur courant, qui en
/// devient l'auteur et obtient automatiquement tous les droits d'édition
/// (voir `Claude.md` § Droits d'édition). Renvoie l'identifiant créé.
#[server]
async fn create_project(
    authority_id: String,
    domain_id: String,
    title: String,
) -> Result<String, ServerFnError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(ServerFnError::new("le titre de l'arrêté est obligatoire"));
    }
    let authority_id: shared::id::ID = authority_id
        .parse()
        .map_err(|_| ServerFnError::new("autorité invalide"))?;
    let domain_id: shared::id::ID = domain_id
        .parse()
        .map_err(|_| ServerFnError::new("domaine invalide"))?;

    let created_by = crate::auth::current_user_id().await?;

    let pool = expect_context::<storage::Pool>();

    // Défense en profondeur : ne pas se fier au seul filtrage côté UI
    // (`list_domains_for_current_user`) pour vérifier le droit sur le domaine.
    let accessible_ids = crate::auth::accessible_domain_ids(&pool, &created_by).await?;
    if !accessible_ids.contains(&domain_id) {
        return Err(ServerFnError::new(
            "vous n'avez pas le droit de créer un projet dans ce domaine",
        ));
    }

    let legal_act = storage::legal_act::create_legal_act(
        &pool,
        shared::model::CreateLegalAct {
            title: title.to_string(),
            domain_id,
            authority_id,
            created_by,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    storage::permission::create_permission(
        &pool,
        shared::model::CreatePermission {
            subject: shared::model::Subject::User(created_by),
            resource_type: "legal_act".to_string(),
            resource: shared::model::ResourceScope::Specific(legal_act.id),
            action: "edit".to_string(),
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    storage::audit_log::record_audit_event(
        &pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(created_by),
            actor_ip: None,
            action: "create".to_string(),
            resource_type: "legal_act".to_string(),
            resource_id: Some(legal_act.id),
            details: None,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;

    Ok(legal_act.id.to_string())
}

#[component]
pub fn PageEditorNew() -> impl IntoView {
    let navigate = use_navigate();

    let (authority_id, set_authority_id) = signal(String::new());
    let (domain_id, set_domain_id) = signal(String::new());
    let (title, set_title) = signal(String::new());
    let (validation_error, set_validation_error) = signal(Option::<String>::None);

    let authorities = Resource::new(|| (), |_| list_authorities());
    let domains = Resource::new(|| (), |_| list_domains_for_current_user());

    let create_action = Action::new(move |input: &(String, String, String)| {
        let (authority_id, domain_id, title) = input.clone();
        create_project(authority_id, domain_id, title)
    });

    Effect::new(move |_| {
        if let Some(Ok(id)) = create_action.value().get() {
            navigate(&format!("/editor/{id}"), NavigateOptions::default());
        }
    });

    let on_submit = move |_| {
        set_validation_error.set(None);
        if authority_id.get().is_empty() {
            set_validation_error.set(Some("Choisissez une autorité.".to_string()));
            return;
        }
        if domain_id.get().is_empty() {
            set_validation_error.set(Some("Choisissez un domaine.".to_string()));
            return;
        }
        if title.get().trim().is_empty() {
            set_validation_error.set(Some("Le titre de l'arrêté est obligatoire.".to_string()));
            return;
        }
        create_action.dispatch((authority_id.get(), domain_id.get(), title.get()));
    };

    let submitting = move || create_action.pending().get();
    let server_error = move || {
        create_action
            .value()
            .get()
            .and_then(|result| result.err())
            .map(|error| error.to_string())
    };

    view! {
        <div class="min-h-screen bg-gray-50 flex items-center justify-center p-6">
            <div class="w-full max-w-lg bg-white border border-gray-200 rounded-sm p-8 flex flex-col gap-6">
                <div>
                    <h1 class="text-xl font-bold text-gray-900">"Nouveau projet d'arrêté"</h1>
                    <p class="text-sm text-gray-600">
                        "Renseignez l'autorité, le domaine et le titre de l'arrêté à rédiger. "
                        "Le domaine ne pourra plus être modifié une fois le projet créé."
                    </p>
                </div>

                {move || validation_error.get().map(|message| view! {
                    <Alert severity=dsfr::components::common::Severity::Error small=true>
                        {message}
                    </Alert>
                })}
                {move || server_error().map(|message| view! {
                    <Alert severity=dsfr::components::common::Severity::Error small=true>
                        {message}
                    </Alert>
                })}

                <div class="flex flex-col gap-4">
                    <Suspense fallback=|| view! { <p class="text-sm text-gray-500">"Chargement des autorités…"</p> }>
                        {move || Suspend::new(async move {
                            let options = authorities.await.unwrap_or_default();
                            let mut select_options = vec![SelectOption::new("", "— Sélectionnez une autorité —")];
                            select_options.extend(options.into_iter().map(|a| SelectOption::new(a.id, a.nom)));
                            view! {
                                <Select
                                    label="Autorité"
                                    options=select_options
                                    value=authority_id
                                    on_change=move |value| set_authority_id.set(value)
                                />
                            }
                        })}
                    </Suspense>

                    <Suspense fallback=|| view! { <p class="text-sm text-gray-500">"Chargement des domaines…"</p> }>
                        {move || Suspend::new(async move {
                            let options = domains.await.unwrap_or_default();
                            // Auto-sélection si l'utilisateur n'a de droit que sur un seul domaine.
                            if options.len() == 1 && domain_id.get_untracked().is_empty() {
                                set_domain_id.set(options[0].id.clone());
                            }
                            let mut select_options = vec![SelectOption::new("", "— Sélectionnez un domaine —")];
                            select_options.extend(options.into_iter().map(|d| SelectOption::new(d.id, d.name)));
                            view! {
                                <Select
                                    label="Domaine"
                                    options=select_options
                                    value=domain_id
                                    on_change=move |value| set_domain_id.set(value)
                                />
                            }
                        })}
                    </Suspense>

                    <Input
                        label="Titre de l'arrêté"
                        value=title
                        on_input=move |value| set_title.set(value)
                    />

                    <Button
                        variant=ButtonVariant::Primary
                        disabled=submitting()
                        on_click=on_submit
                    >
                        {move || if submitting() { "Création…" } else { "Créer le projet" }}
                    </Button>
                </div>
            </div>
        </div>
    }
}
