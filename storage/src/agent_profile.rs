//! Catalogue des profils d'agents experts éphémères (voir migration
//! `0015_agent_profiles`), éditable depuis `/admin/agent-profiles` et lu par
//! `server::editor::ports::StorageAgentCatalog` (implémentation de
//! `agent::catalog::AgentCatalog`).

use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{AgentProfile, AgentProfileChangeset, CreateAgentProfile};

fn from_row(row: PgRow) -> Result<AgentProfile, StorageError> {
    Ok(AgentProfile {
        id: id::column(&row, "id")?,
        name: row.try_get("name")?,
        display_name: row.try_get("display_name")?,
        system_prompt: row.try_get("system_prompt")?,
        tool_names: row.try_get("tool_names")?,
        max_steps: row.try_get("max_steps")?,
        enabled: row.try_get("enabled")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Enregistre un profil d'agent expert.
pub async fn create_agent_profile(
    pool: &Pool,
    args: CreateAgentProfile,
) -> Result<AgentProfile, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO agent_profiles (id, name, display_name, system_prompt, tool_names, max_steps) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(args.name)
    .bind(args.display_name)
    .bind(args.system_prompt)
    .bind(args.tool_names)
    .bind(args.max_steps)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère un profil d'agent expert par son identifiant.
pub async fn get_agent_profile(pool: &Pool, profile_id: &ID) -> Result<AgentProfile, StorageError> {
    let row = sqlx::query("SELECT * FROM agent_profiles WHERE id = $1")
        .bind(id::encode(profile_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Récupère un profil d'agent expert *activé* par son nom technique, pour la
/// résolution d'une délégation (voir `agent::catalog::AgentCatalog::get`).
/// `Ok(None)` si aucun profil activé ne porte ce nom (supprimé, désactivé,
/// ou jamais existé).
pub async fn get_enabled_agent_profile_by_name(
    pool: &Pool,
    name: &str,
) -> Result<Option<AgentProfile>, StorageError> {
    let row = sqlx::query("SELECT * FROM agent_profiles WHERE name = $1 AND enabled")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    row.map(from_row).transpose()
}

/// Liste l'ensemble des profils d'agents experts, y compris désactivés (pour
/// l'écran d'administration).
pub async fn list_agent_profiles(pool: &Pool) -> Result<Vec<AgentProfile>, StorageError> {
    let rows = sqlx::query("SELECT * FROM agent_profiles ORDER BY name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Liste les seuls profils *activés*, pour construire le schéma de l'outil
/// `delegate_to_expert` (voir `agent::catalog::AgentCatalog::list`).
pub async fn list_enabled_agent_profiles(pool: &Pool) -> Result<Vec<AgentProfile>, StorageError> {
    let rows = sqlx::query("SELECT * FROM agent_profiles WHERE enabled ORDER BY name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie des attributs d'un profil d'agent expert.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle.
pub async fn update_agent_profile(
    pool: &Pool,
    profile_id: &ID,
    changeset: AgentProfileChangeset,
) -> Result<AgentProfile, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE agent_profiles SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(name) = changeset.name {
        set.push("name = ").push_bind_unseparated(name);
    }
    if let Some(display_name) = changeset.display_name {
        set.push("display_name = ")
            .push_bind_unseparated(display_name);
    }
    if let Some(system_prompt) = changeset.system_prompt {
        set.push("system_prompt = ")
            .push_bind_unseparated(system_prompt);
    }
    if let Some(tool_names) = changeset.tool_names {
        set.push("tool_names = ").push_bind_unseparated(tool_names);
    }
    if let Some(max_steps) = changeset.max_steps {
        set.push("max_steps = ").push_bind_unseparated(max_steps);
    }
    if let Some(enabled) = changeset.enabled {
        set.push("enabled = ").push_bind_unseparated(enabled);
    }
    builder
        .push(" WHERE id = ")
        .push_bind(profile_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Supprime un profil d'agent expert.
pub async fn delete_agent_profile(pool: &Pool, profile_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM agent_profiles WHERE id = $1")
        .bind(id::encode(profile_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
