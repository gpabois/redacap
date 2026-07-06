//! Page `/` : tableau de bord affiché à l'utilisateur authentifié, listant
//! les projets d'arrêtés auxquels il a accès (auteur, ou droit direct/hérité
//! de ses groupes) — voir `Claude.md` § Pages de l'application.
//!
//! Redirige vers `/login` si l'utilisateur n'est pas authentifié : la
//! résolution de session échoue dans `dashboard_projects`, ce qui déclenche
//! `leptos_axum::redirect` avant de renvoyer l'erreur (voir la documentation
//! de ce module côté SSR pour le mécanisme de redirection).

use dsfr::{Badge, Header, Severity, Table};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Identité affichée dans la zone outils de l'en-tête : bulle d'avatar menant
/// à `/account`, et lien vers `/admin` si administrateur (voir `Claude.md`
/// § Modèle de permissions et [`crate::auth::HeaderIdentity`]).
#[server]
async fn dashboard_header_summary() -> Result<crate::auth::HeaderIdentity, ServerFnError> {
    let user_id = match crate::auth::current_user_id().await {
        Ok(user_id) => user_id,
        Err(error) => {
            leptos_axum::redirect("/login");
            return Err(error);
        }
    };

    let pool = expect_context::<storage::Pool>();
    crate::auth::header_identity(&pool, &user_id).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectSummary {
    id: String,
    title: String,
    status_label: String,
    status_severity_index: u8,
    authority_name: String,
    domain_name: String,
    updated_at: String,
}

#[cfg(feature = "ssr")]
fn status_label_and_severity(status: shared::model::LegalActStatus) -> (&'static str, u8) {
    match status {
        shared::model::LegalActStatus::Redaction => ("Rédaction", 0),
        shared::model::LegalActStatus::Verification => ("Vérification", 1),
        shared::model::LegalActStatus::Approbation => ("Approbation", 1),
        shared::model::LegalActStatus::Finalise => ("Finalisé", 2),
    }
}

fn severity_from_index(index: u8) -> Severity {
    match index {
        0 => Severity::Info,
        1 => Severity::Warning,
        _ => Severity::Success,
    }
}

#[server]
async fn dashboard_projects() -> Result<Vec<ProjectSummary>, ServerFnError> {
    let user_id = match crate::auth::current_user_id().await {
        Ok(user_id) => user_id,
        Err(error) => {
            leptos_axum::redirect("/login");
            return Err(error);
        }
    };

    let pool = expect_context::<storage::Pool>();
    let accessible_ids = crate::auth::accessible_legal_act_ids(&pool, &user_id).await?;
    let legal_acts = storage::legal_act::list_legal_acts_for_user(&pool, &user_id, &accessible_ids)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let mut summaries = Vec::with_capacity(legal_acts.len());
    for legal_act in legal_acts {
        let authority_name = storage::authority::get_authority(&pool, &legal_act.authority_id)
            .await
            .map(|authority| authority.nom)
            .unwrap_or_else(|_| "Autorité inconnue".to_string());
        let domain_name = storage::domain::get_domain(&pool, &legal_act.domain_id)
            .await
            .map(|domain| domain.name)
            .unwrap_or_else(|_| "Domaine inconnu".to_string());
        let (status_label, status_severity_index) = status_label_and_severity(legal_act.status);

        summaries.push(ProjectSummary {
            id: legal_act.id.to_string(),
            title: legal_act.title,
            status_label: status_label.to_string(),
            status_severity_index,
            authority_name,
            domain_name,
            updated_at: legal_act.updated_at.format("%d/%m/%Y %H:%M").to_string(),
        });
    }

    Ok(summaries)
}

#[component]
pub fn PageDashboard() -> impl IntoView {
    let projects = Resource::new(|| (), |_| dashboard_projects());
    let user_summary = Resource::new(|| (), |_| dashboard_header_summary());

    view! {
        <div class="min-h-screen bg-gray-50 dark:bg-gray-800">
            <Header service_title="Redac'AP" service_tagline="Éditeur d'arrêtés préfectoraux">
                <Suspense fallback=|| ()>
                    {move || Suspend::new(async move {
                        match user_summary.await {
                            Err(_) => None,
                            Ok(user) => Some(view! {
                                {user.is_admin.then(|| view! {
                                    <a
                                        href="/admin"
                                        class="text-sm font-bold text-blue-france dark:text-blue-france-925 hover:underline whitespace-nowrap"
                                    >
                                        "Administration"
                                    </a>
                                })}
                                <a
                                    href="/account"
                                    title="Mon compte"
                                    class="flex items-center justify-center w-9 h-9 rounded-full bg-blue-france text-white font-bold hover:bg-blue-france-hover transition-colors shrink-0"
                                >
                                    {user.initial}
                                </a>
                            }),
                        }
                    })}
                </Suspense>
            </Header>
            <div class="max-w-6xl mx-auto p-6 flex flex-col gap-6">
                <div class="flex items-center justify-between flex-wrap gap-3">
                    <div>
                        <h1 class="text-xl font-bold text-gray-900 dark:text-gray-100">"Mes projets d'arrêtés"</h1>
                        <p class="text-sm text-gray-600 dark:text-gray-400">
                            "Retrouvez ici les projets d'arrêtés que vous avez créés ou auxquels vous avez été associé."
                        </p>
                    </div>
                    <a
                        href="/editor/new"
                        class="bg-blue-france text-white hover:bg-blue-france-hover font-bold px-4 py-2 transition-colors inline-flex items-center"
                    >
                        "Nouveau projet"
                    </a>
                </div>

                <Suspense fallback=|| view! { <p class="text-gray-500 dark:text-gray-400">"Chargement des projets…"</p> }>
                    {move || Suspend::new(async move {
                        match projects.await {
                            Err(_) => view! { <p class="text-gray-500 dark:text-gray-400">"Redirection…"</p> }.into_any(),
                            Ok(projects) if projects.is_empty() => view! {
                                <p class="text-gray-500 dark:text-gray-400">
                                    "Aucun projet d'arrêté pour l'instant. Créez-en un pour commencer."
                                </p>
                            }.into_any(),
                            Ok(projects) => view! {
                                <Table headers=vec!["Titre", "Statut", "Autorité", "Domaine", "Dernière modification", ""]>
                                    {projects.into_iter().map(|project| {
                                        let href = format!("/editor/{}", project.id);
                                        view! {
                                            <tr class="align-top">
                                                <td class="px-3 py-2 font-bold">{project.title}</td>
                                                <td class="px-3 py-2">
                                                    <Badge severity=severity_from_index(project.status_severity_index) small=true>
                                                        {project.status_label}
                                                    </Badge>
                                                </td>
                                                <td class="px-3 py-2">{project.authority_name}</td>
                                                <td class="px-3 py-2">{project.domain_name}</td>
                                                <td class="px-3 py-2 whitespace-nowrap">{project.updated_at}</td>
                                                <td class="px-3 py-2">
                                                    <a href=href class="text-blue-france dark:text-blue-france-925 font-bold hover:underline">
                                                        "Ouvrir"
                                                    </a>
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </Table>
                            }.into_any(),
                        }
                    })}
                </Suspense>
            </div>
        </div>
    }
}
