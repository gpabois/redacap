//! Panneau administrateur : tableau de bord, gestion des comptes, groupes,
//! autorités administratives, domaines/intentions, outils de l'agent IA,
//! fournisseurs OIDC et consultation du journal d'audit — voir `Claude.md`
//! § Pages de l'application (domaine `/admin`).
//!
//! Chaque page vérifie l'accès via [`admin_context`] avant d'afficher son
//! contenu (voir `app::auth::require_admin`).

pub mod agent_tools;
pub mod ai_models;
pub mod audit;
pub mod authorities;
pub mod dashboard;
pub mod domains;
pub mod groups;
pub mod integrations;
pub mod intentions;
pub mod oidc;
pub mod permissions;
pub mod users;

pub use agent_tools::PageAdminAgentTools;
pub use ai_models::PageAdminAiModels;
pub use audit::PageAdminAudit;
pub use authorities::PageAdminAuthorities;
pub use dashboard::PageAdminDashboard;
pub use domains::PageAdminDomains;
pub use groups::PageAdminGroups;
pub use integrations::PageAdminIntegrations;
pub use intentions::PageAdminIntentions;
pub use oidc::PageAdminOidc;
pub use permissions::PermissionsPanel;
pub use users::PageAdminUsers;

use dsfr::{Alert, Button, ButtonVariant, Header, Severity};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Contexte d'accès résolu pour l'utilisateur courant, transmis aux panneaux
/// de chaque page pour conditionner l'affichage des contrôles réservés aux
/// super administrateurs (ex. attribution des droits `administrateur`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminAccess {
    pub is_super_admin: bool,
    /// Initiale du nom affiché de l'utilisateur courant, pour la bulle
    /// d'avatar de [`AdminHeader`].
    pub initial: String,
}

/// Vérifie que l'utilisateur courant a accès au panneau administrateur.
/// Échoue sinon (voir `app::auth::require_admin`) : les pages affichent alors
/// un message d'accès refusé plutôt que leur contenu.
#[server]
pub async fn admin_context() -> Result<AdminAccess, ServerFnError> {
    let user_id = crate::auth::current_user_id().await?;
    let pool = expect_context::<storage::Pool>();
    let is_super_admin = crate::auth::require_admin(&pool, &user_id).await?;
    let user = storage::user::get_user(&pool, &user_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let initial = crate::auth::display_initial(&user.display_name);
    Ok(AdminAccess {
        is_super_admin,
        initial,
    })
}

/// Section active du panneau administrateur, pour la mise en surbrillance du
/// lien correspondant dans [`AdminNav`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminSection {
    Dashboard,
    Users,
    Groups,
    Authorities,
    Domains,
    Intentions,
    AgentTools,
    AiModels,
    Oidc,
    Integrations,
    Audit,
}

/// En-tête DSFR du panneau administrateur : bulle d'avatar (initiale du nom
/// affiché de l'utilisateur courant) menant à `/account`, et lien de retour
/// vers le tableau de bord `/` (voir `crate::pages::dashboard::PageDashboard`).
#[component]
pub fn AdminHeader(initial: String) -> impl IntoView {
    view! {
        <Header service_title="Redac'AP" service_tagline="Panneau d'administration">
            <a
                href="/"
                class="text-sm font-bold text-blue-france hover:underline whitespace-nowrap"
            >
                "Tableau de bord"
            </a>
            <a
                href="/account"
                title="Mon compte"
                class="flex items-center justify-center w-9 h-9 rounded-full bg-blue-france text-white font-bold hover:bg-blue-france-hover transition-colors shrink-0"
            >
                {initial}
            </a>
        </Header>
    }
}

/// Navigation entre les pages du panneau administrateur. Simples liens
/// `<a>` (et non des onglets côté client) : chaque section est une route
/// distincte avec ses propres données.
#[component]
pub fn AdminNav(active: AdminSection) -> impl IntoView {
    let link_class = |section: AdminSection| {
        if section == active {
            "px-4 py-2 text-sm font-bold border-b-2 border-blue-france text-blue-france whitespace-nowrap"
        } else {
            "px-4 py-2 text-sm font-bold border-b-2 border-transparent text-gray-600 hover:text-blue-france whitespace-nowrap"
        }
    };
    view! {
        <nav class="border-b border-gray-300 bg-white">
            <div class="max-w-6xl mx-auto flex overflow-x-auto">
                <a href="/admin" class=link_class(AdminSection::Dashboard)>"Tableau de bord"</a>
                <a href="/admin/users" class=link_class(AdminSection::Users)>"Utilisateurs"</a>
                <a href="/admin/groups" class=link_class(AdminSection::Groups)>"Groupes"</a>
                <a href="/admin/authorities" class=link_class(AdminSection::Authorities)>"Autorités"</a>
                <a href="/admin/domains" class=link_class(AdminSection::Domains)>"Domaines"</a>
                <a href="/admin/intentions" class=link_class(AdminSection::Intentions)>"Intentions"</a>
                <a href="/admin/agent-tools" class=link_class(AdminSection::AgentTools)>"Outils de l'agent"</a>
                <a href="/admin/ai-models" class=link_class(AdminSection::AiModels)>"Modèles IA"</a>
                <a href="/admin/oidc" class=link_class(AdminSection::Oidc)>"Fournisseurs OIDC"</a>
                <a href="/admin/integrations" class=link_class(AdminSection::Integrations)>"Intégrations"</a>
                <a href="/admin/audit" class=link_class(AdminSection::Audit)>"Journal d'audit"</a>
            </div>
        </nav>
    }
}

/// Trace une action de mutation du panneau administrateur dans le journal
/// d'audit (voir contrainte racine « Audit log »). Partagé par les server
/// fns de `users`, `groups` et `oidc` : toute action sensible doit être
/// tracée avec horodatage, identité et IP.
#[cfg(feature = "ssr")]
pub(crate) async fn record_admin_audit_event(
    pool: &storage::Pool,
    actor_id: shared::id::ID,
    action: &str,
    resource_type: &str,
    resource_id: Option<shared::id::ID>,
) -> Result<(), ServerFnError> {
    let actor_ip = crate::auth::current_actor_ip().await;
    storage::audit_log::record_audit_event(
        pool,
        shared::model::CreateAuditEvent {
            actor_id: Some(actor_id),
            actor_ip,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            details: None,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(())
}

/// Bouton nécessitant un second clic pour confirmer une action destructive
/// (suppression) — évite d'introduire un composant modal dédié pour ce seul
/// besoin.
#[component]
pub fn ConfirmButton(
    label: &'static str,
    confirm_label: &'static str,
    on_confirm: Callback<()>,
    #[prop(optional)] disabled: bool,
) -> impl IntoView {
    let (confirming, set_confirming) = signal(false);
    view! {
        <Button
            variant=ButtonVariant::Secondary
            disabled=disabled
            on_click=move |_| {
                if confirming.get_untracked() {
                    set_confirming.set(false);
                    on_confirm.run(());
                } else {
                    set_confirming.set(true);
                }
            }
        >
            {move || if confirming.get() { confirm_label } else { label }}
        </Button>
    }
}

/// Message affiché sur une page `/admin/*` lorsque [`admin_context`] échoue.
#[component]
pub fn AdminAccessDenied() -> impl IntoView {
    view! {
        <div class="min-h-screen bg-gray-50 flex items-center justify-center p-6">
            <div class="max-w-md">
                <Alert severity=Severity::Error>
                    "Accès réservé aux administrateurs."
                </Alert>
            </div>
        </div>
    }
}
