//! Résolution de l'utilisateur courant côté serveur, à partir du cookie de
//! session opaque posé par `server::auth::session::start_session`.
//!
//! Compilé uniquement côté serveur (SSR) : le corps des fonctions `#[server]`
//! qui l'utilisent est de toute façon retiré côté client par le macro
//! `#[server]`, mais ce module référence directement des crates ssr-only
//! (`axum`, `axum-extra`, `storage`), absentes de la compilation `hydrate`.

#[cfg(feature = "ssr")]
use axum_extra::extract::cookie::{Key, PrivateCookieJar};
#[cfg(feature = "ssr")]
use leptos::prelude::*;
#[cfg(feature = "ssr")]
use shared::id::ID;
#[cfg(feature = "ssr")]
use shared::model::{
    ACTION_ADMINISTRATEUR, ACTION_SUPER_ADMINISTRATEUR, Permission, ResourceScope,
};
#[cfg(feature = "ssr")]
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Identité affichée dans la bulle d'avatar des en-têtes DSFR de
/// l'application (tableau de bord, panneau admin, éditeur) : initiale du nom
/// affiché (voir [`display_initial`]) et accès ou non au panneau admin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderIdentity {
    pub user_id: String,
    pub initial: String,
    pub is_admin: bool,
}

/// Réduit un nom affiché à son initiale capitalisée, pour les bulles
/// d'avatar de l'en-tête.
#[cfg(feature = "ssr")]
pub fn display_initial(display_name: &str) -> String {
    display_name
        .chars()
        .next()
        .map(|letter| letter.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Résout l'identité affichée dans la bulle d'avatar de l'en-tête pour
/// `user_id` (voir [`HeaderIdentity`]) — partagé par le tableau de bord et
/// l'éditeur (le panneau admin a ses propres besoins, voir
/// `pages::admin::admin_context`).
#[cfg(feature = "ssr")]
pub async fn header_identity(
    pool: &storage::Pool,
    user_id: &ID,
) -> Result<HeaderIdentity, ServerFnError> {
    let user = storage::user::get_user(pool, user_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let is_admin = is_administrator(pool, user_id).await?;
    Ok(HeaderIdentity {
        user_id: user_id.to_string(),
        initial: display_initial(&user.display_name),
        is_admin,
    })
}

/// Nom du cookie de session, en miroir de `server::auth::session::COOKIE_NAME`.
#[cfg(feature = "ssr")]
const SESSION_COOKIE_NAME: &str = "session";

/// Identifiant de l'utilisateur courant, déduit du cookie de session envoyé
/// avec la requête. Échoue si le cookie est absent, invalide, ou si la
/// session est expirée.
#[cfg(feature = "ssr")]
pub async fn current_user_id() -> Result<ID, ServerFnError> {
    let headers = leptos_axum::extract::<axum::http::HeaderMap>()
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))?;
    let key = expect_context::<Key>();
    let jar = PrivateCookieJar::from_headers(&headers, key);

    let session_id = jar
        .get(SESSION_COOKIE_NAME)
        .ok_or_else(|| ServerFnError::new("session absente ou invalide"))?
        .value()
        .parse::<ID>()
        .map_err(|_| ServerFnError::new("identifiant de session invalide"))?;

    let pool = expect_context::<storage::Pool>();
    let session = storage::session::get_active_session(&pool, &session_id)
        .await
        .map_err(|_| ServerFnError::new("session expirée, veuillez vous reconnecter"))?;
    Ok(session.user_id)
}

/// Adresse IP de l'auteur de la requête courante, à des fins d'audit
/// (voir contrainte racine « Audit log »). Best-effort : lit l'en-tête
/// `X-Forwarded-For` (posé par le reverse proxy en déploiement), `None` si
/// absent plutôt que de faire échouer l'action tracée.
#[cfg(feature = "ssr")]
pub async fn current_actor_ip() -> Option<String> {
    let headers = leptos_axum::extract::<axum::http::HeaderMap>().await.ok()?;
    let value = headers.get("x-forwarded-for")?.to_str().ok()?;
    value.split(',').next().map(|ip| ip.trim().to_string())
}

/// Permissions effectives d'un utilisateur : ses droits directs, ceux de
/// chacun des groupes dont il est membre, et ceux de tous les descendants de
/// ces groupes (une entité hérite des droits de ses descendants — voir
/// `Claude.md` § Autorisation).
#[cfg(feature = "ssr")]
pub async fn effective_permissions(
    pool: &storage::Pool,
    user_id: &ID,
) -> Result<Vec<Permission>, ServerFnError> {
    let mut permissions = storage::permission::list_permissions_for_user(pool, user_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let direct_group_ids = storage::user_group::list_groups_for_user(pool, user_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;

    let mut seen = HashSet::new();
    for group_id in direct_group_ids {
        if !seen.insert(group_id) {
            continue;
        }
        permissions.extend(
            storage::permission::list_permissions_for_group(pool, &group_id)
                .await
                .map_err(|error| ServerFnError::new(error.to_string()))?,
        );
        let descendants = storage::group::list_descendant_groups(pool, &group_id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
        for descendant in descendants {
            if !seen.insert(descendant.id) {
                continue;
            }
            permissions.extend(
                storage::permission::list_permissions_for_group(pool, &descendant.id)
                    .await
                    .map_err(|error| ServerFnError::new(error.to_string()))?,
            );
        }
    }

    Ok(permissions)
}

/// Identifiants des actes légaux sur lesquels l'utilisateur dispose d'un
/// droit direct ou hérité de ses groupes (permission `resource_type ==
/// "legal_act"` à portée `Specific`) — utilisé par le tableau de bord `/`
/// en complément des projets dont l'utilisateur est l'auteur (couverts
/// séparément par `created_by`, voir `storage::legal_act::list_legal_acts_for_user`).
#[cfg(feature = "ssr")]
pub async fn accessible_legal_act_ids(
    pool: &storage::Pool,
    user_id: &ID,
) -> Result<Vec<ID>, ServerFnError> {
    let permissions = effective_permissions(pool, user_id).await?;
    Ok(permissions
        .into_iter()
        .filter(|permission| permission.resource_type == "legal_act")
        .filter_map(|permission| match permission.resource {
            ResourceScope::Specific(id) => Some(id),
            _ => None,
        })
        .collect())
}

/// Identifiants des domaines sur lesquels l'utilisateur dispose d'un droit
/// direct ou hérité de ses groupes (permission `resource_type == "domain"` à
/// portée `Specific`, action `"use"`) — seuls ces domaines sont proposés à la
/// création d'un projet (voir `Claude.md` § « Ajoute aux projets... »,
/// `app::pages::editor_new`).
#[cfg(feature = "ssr")]
pub async fn accessible_domain_ids(
    pool: &storage::Pool,
    user_id: &ID,
) -> Result<Vec<ID>, ServerFnError> {
    let permissions = effective_permissions(pool, user_id).await?;
    Ok(permissions
        .into_iter()
        .filter(|permission| permission.resource_type == "domain" && permission.action == "use")
        .filter_map(|permission| match permission.resource {
            ResourceScope::Specific(id) => Some(id),
            _ => None,
        })
        .collect())
}

/// Indique si `permissions` contient une permission globale portant l'action donnée.
#[cfg(feature = "ssr")]
pub(crate) fn has_global_action(permissions: &[Permission], action: &str) -> bool {
    permissions.iter().any(|permission| {
        permission.resource == ResourceScope::Global && permission.action == action
    })
}

/// Indique si l'utilisateur possède le droit spécial `administrateur` ou
/// `super_administrateur` (le second implique le premier).
#[cfg(feature = "ssr")]
pub async fn is_administrator(pool: &storage::Pool, user_id: &ID) -> Result<bool, ServerFnError> {
    let permissions = effective_permissions(pool, user_id).await?;
    Ok(has_global_action(&permissions, ACTION_ADMINISTRATEUR)
        || has_global_action(&permissions, ACTION_SUPER_ADMINISTRATEUR))
}

/// Indique si l'utilisateur possède le droit spécial `super_administrateur`.
#[cfg(feature = "ssr")]
pub async fn is_super_administrator(
    pool: &storage::Pool,
    user_id: &ID,
) -> Result<bool, ServerFnError> {
    let permissions = effective_permissions(pool, user_id).await?;
    Ok(has_global_action(&permissions, ACTION_SUPER_ADMINISTRATEUR))
}

/// Vérifie que l'utilisateur a accès au panneau administrateur, et renvoie
/// `true` s'il est `super_administrateur` (droits étendus, ex. gestion des
/// droits `administrateur`/`super_administrateur` eux-mêmes).
#[cfg(feature = "ssr")]
pub async fn require_admin(pool: &storage::Pool, user_id: &ID) -> Result<bool, ServerFnError> {
    let permissions = effective_permissions(pool, user_id).await?;
    if has_global_action(&permissions, ACTION_SUPER_ADMINISTRATEUR) {
        return Ok(true);
    }
    if has_global_action(&permissions, ACTION_ADMINISTRATEUR) {
        return Ok(false);
    }
    Err(ServerFnError::new("accès réservé aux administrateurs"))
}

/// Vérifie que l'utilisateur possède le droit spécial `super_administrateur`,
/// requis pour accorder ou révoquer les droits `administrateur`/
/// `super_administrateur` eux-mêmes (voir `Claude.md` § Autorisation : un
/// `administrateur` simple ne peut pas retirer les droits d'un autre
/// administrateur ou super administrateur).
#[cfg(feature = "ssr")]
pub async fn require_super_admin(pool: &storage::Pool, user_id: &ID) -> Result<(), ServerFnError> {
    if is_super_administrator(pool, user_id).await? {
        Ok(())
    } else {
        Err(ServerFnError::new(
            "action réservée aux super administrateurs",
        ))
    }
}

/// Indique si `action` désigne un droit de niveau administrateur, dont
/// l'attribution/révocation est réservée aux super administrateurs.
#[cfg(feature = "ssr")]
pub fn is_admin_tier_action(action: &str) -> bool {
    action == ACTION_ADMINISTRATEUR || action == ACTION_SUPER_ADMINISTRATEUR
}
