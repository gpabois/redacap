use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::AgentToolScope;

fn from_row(row: PgRow) -> Result<AgentToolScope, StorageError> {
    Ok(AgentToolScope {
        tool_name: row.try_get("tool_name")?,
        domain_id: id::column_opt(&row, "domain_id")?,
    })
}

/// Liste l'ensemble des portées de disponibilité configurées, pour l'affichage
/// du panneau administrateur (voir `agent::tools::CONFIGURABLE_TOOLS` pour le
/// catalogue des outils concernés).
pub async fn list_agent_tool_scopes(pool: &Pool) -> Result<Vec<AgentToolScope>, StorageError> {
    let rows = sqlx::query("SELECT * FROM agent_tool_scopes ORDER BY tool_name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Active ou désactive la disponibilité globale d'un outil.
pub async fn set_tool_global(
    pool: &Pool,
    tool_name: &str,
    enabled: bool,
) -> Result<(), StorageError> {
    if enabled {
        sqlx::query(
            "INSERT INTO agent_tool_scopes (tool_name, domain_id) VALUES ($1, NULL) \
             ON CONFLICT DO NOTHING",
        )
        .bind(tool_name)
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM agent_tool_scopes WHERE tool_name = $1 AND domain_id IS NULL")
            .bind(tool_name)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Active ou désactive la disponibilité d'un outil pour un domaine précis.
pub async fn set_tool_domain(
    pool: &Pool,
    tool_name: &str,
    domain_id: &ID,
    enabled: bool,
) -> Result<(), StorageError> {
    if enabled {
        sqlx::query(
            "INSERT INTO agent_tool_scopes (tool_name, domain_id) VALUES ($1, $2) \
             ON CONFLICT DO NOTHING",
        )
        .bind(tool_name)
        .bind(id::encode(domain_id))
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM agent_tool_scopes WHERE tool_name = $1 AND domain_id = $2")
            .bind(tool_name)
            .bind(id::encode(domain_id))
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Noms des outils disponibles pour un domaine : ceux configurés globalement,
/// plus ceux spécifiquement réservés à ce domaine — utilisé par `server` pour
/// filtrer les outils enregistrés dans la boucle agentique.
pub async fn list_allowed_tool_names_for_domain(
    pool: &Pool,
    domain_id: &ID,
) -> Result<Vec<String>, StorageError> {
    let rows = sqlx::query(
        "SELECT DISTINCT tool_name FROM agent_tool_scopes WHERE domain_id IS NULL OR domain_id = $1",
    )
    .bind(id::encode(domain_id))
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| row.try_get("tool_name").map_err(StorageError::from))
        .collect()
}
