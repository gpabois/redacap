use sqlx::postgres::PgRow;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{AiModel, AiModelChangeset, CreateAiModel};

fn from_row(row: PgRow) -> Result<AiModel, StorageError> {
    Ok(AiModel {
        id: id::column(&row, "id")?,
        name: row.try_get("name")?,
        base_url: row.try_get("base_url")?,
        model: row.try_get("model")?,
        api_key_encrypted: row.try_get("api_key_encrypted")?,
        system_prompt: row.try_get("system_prompt")?,
        active: row.try_get("active")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Enregistre un modèle IA (moteur potentiel de l'agent « Marie »).
pub async fn create_ai_model(pool: &Pool, args: CreateAiModel) -> Result<AiModel, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO ai_models (id, name, base_url, model, api_key_encrypted, system_prompt) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(args.name)
    .bind(args.base_url)
    .bind(args.model)
    .bind(args.api_key_encrypted)
    .bind(args.system_prompt)
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Récupère un modèle IA par son identifiant.
pub async fn get_ai_model(pool: &Pool, model_id: &ID) -> Result<AiModel, StorageError> {
    let row = sqlx::query("SELECT * FROM ai_models WHERE id = $1")
        .bind(id::encode(model_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Récupère le modèle IA actuellement utilisé comme moteur de l'agent
/// « Marie », s'il en existe un (voir `ai_models_single_active_idx`).
pub async fn get_active_ai_model(pool: &Pool) -> Result<Option<AiModel>, StorageError> {
    let row = sqlx::query("SELECT * FROM ai_models WHERE active")
        .fetch_optional(pool)
        .await?;
    row.map(from_row).transpose()
}

/// Liste l'ensemble des modèles IA enregistrés.
pub async fn list_ai_models(pool: &Pool) -> Result<Vec<AiModel>, StorageError> {
    let rows = sqlx::query("SELECT * FROM ai_models ORDER BY name")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(from_row).collect()
}

/// Met à jour tout ou partie des attributs d'un modèle IA.
///
/// Seuls les champs à `Some(_)` du changeset sont modifiés ; les autres conservent
/// leur valeur actuelle. L'activation se fait via [`set_active_ai_model`], pas ici.
pub async fn update_ai_model(
    pool: &Pool,
    model_id: &ID,
    changeset: AiModelChangeset,
) -> Result<AiModel, StorageError> {
    let mut builder = QueryBuilder::<Postgres>::new("UPDATE ai_models SET ");
    let mut set = builder.separated(", ");
    set.push("updated_at = now()");
    if let Some(name) = changeset.name {
        set.push("name = ").push_bind_unseparated(name);
    }
    if let Some(base_url) = changeset.base_url {
        set.push("base_url = ").push_bind_unseparated(base_url);
    }
    if let Some(model) = changeset.model {
        set.push("model = ").push_bind_unseparated(model);
    }
    if let Some(api_key_encrypted) = changeset.api_key_encrypted {
        set.push("api_key_encrypted = ")
            .push_bind_unseparated(api_key_encrypted);
    }
    if let Some(system_prompt) = changeset.system_prompt {
        set.push("system_prompt = ")
            .push_bind_unseparated(system_prompt);
    }
    builder
        .push(" WHERE id = ")
        .push_bind(model_id.as_bytes().to_vec());
    builder.push(" RETURNING *");

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Désigne `model_id` comme unique modèle actif (moteur de l'agent « Marie »),
/// en désactivant tout autre modèle dans la même transaction (voir
/// `ai_models_single_active_idx`, qui interdit plus d'une ligne active).
pub async fn set_active_ai_model(pool: &Pool, model_id: &ID) -> Result<AiModel, StorageError> {
    let mut tx = pool.begin().await?;

    sqlx::query("UPDATE ai_models SET active = false, updated_at = now() WHERE active")
        .execute(&mut *tx)
        .await?;

    let row = sqlx::query(
        "UPDATE ai_models SET active = true, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id::encode(model_id))
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(StorageError::NotFound)?;

    tx.commit().await?;
    from_row(row)
}

/// Supprime un modèle IA.
pub async fn delete_ai_model(pool: &Pool, model_id: &ID) -> Result<(), StorageError> {
    let result = sqlx::query("DELETE FROM ai_models WHERE id = $1")
        .bind(id::encode(model_id))
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}
