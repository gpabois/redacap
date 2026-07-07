//! Persistance du CRDT Yrs des commentaires/notes de travail
//! (`legal_act::Review`) d'un acte légal, en tout point symétrique à
//! [`crate::legal_act`] pour le corps de l'acte (journal `updates` +
//! instantané consolidé, voir `storage::CLAUDE.md` § Actes légaux — CRDT
//! Yrs), mais dans des tables dédiées : les deux documents Yrs sont
//! indépendants et n'ont pas la même cadence de mise à jour.

use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{
    CreateLegalActReviewUpdate, LegalActReviewSnapshot, LegalActReviewSnapshotConsolidation,
    LegalActReviewUpdate,
};

fn update_from_row(row: PgRow) -> Result<LegalActReviewUpdate, StorageError> {
    Ok(LegalActReviewUpdate {
        legal_act_id: id::column(&row, "legal_act_id")?,
        seq: row.try_get("seq")?,
        update: row.try_get("update")?,
        author_id: id::column(&row, "author_id")?,
        created_at: row.try_get("created_at")?,
    })
}

fn snapshot_from_row(row: PgRow) -> Result<LegalActReviewSnapshot, StorageError> {
    Ok(LegalActReviewSnapshot {
        legal_act_id: id::column(&row, "legal_act_id")?,
        snapshot: row.try_get("snapshot")?,
        seq: row.try_get("seq")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Journalise une mise à jour Yrs incrémentale du document de commentaires
/// (append-only, aucune transformation).
pub async fn append_update(
    pool: &Pool,
    args: CreateLegalActReviewUpdate,
) -> Result<LegalActReviewUpdate, StorageError> {
    let row = sqlx::query(
        "INSERT INTO legal_act_review_updates (legal_act_id, seq, update, author_id) \
         VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(id::encode(&args.legal_act_id))
    .bind(args.seq)
    .bind(args.update)
    .bind(id::encode(&args.author_id))
    .fetch_one(pool)
    .await?;
    update_from_row(row)
}

/// Liste les mises à jour postérieures à `since_seq`, dans l'ordre de séquence.
///
/// Combinée au dernier snapshot via [`get_snapshot`], permet de reconstruire le `Doc` Yrs.
pub async fn list_updates_since(
    pool: &Pool,
    legal_act_id: &ID,
    since_seq: i64,
) -> Result<Vec<LegalActReviewUpdate>, StorageError> {
    let rows = sqlx::query(
        "SELECT * FROM legal_act_review_updates WHERE legal_act_id = $1 AND seq > $2 ORDER BY seq",
    )
    .bind(id::encode(legal_act_id))
    .bind(since_seq)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(update_from_row).collect()
}

/// Récupère le dernier instantané consolidé des commentaires d'un acte légal, s'il existe.
pub async fn get_snapshot(
    pool: &Pool,
    legal_act_id: &ID,
) -> Result<Option<LegalActReviewSnapshot>, StorageError> {
    let row = sqlx::query("SELECT * FROM legal_act_review_snapshots WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .fetch_optional(pool)
        .await?;
    row.map(snapshot_from_row).transpose()
}

/// Identifiants des actes légaux ayant au moins une mise à jour de
/// commentaires en attente de consolidation (voir [`consolidate_snapshot`]) —
/// utilisé par `worker` pour cibler sa tâche périodique de consolidation.
pub async fn list_legal_acts_with_pending_updates(pool: &Pool) -> Result<Vec<ID>, StorageError> {
    let rows = sqlx::query("SELECT DISTINCT legal_act_id FROM legal_act_review_updates")
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| id::column(row, "legal_act_id"))
        .collect()
}

/// Consolide un instantané des commentaires et purge le journal d'updates
/// devenu redondant, atomiquement. Voir
/// `crate::legal_act::consolidate_snapshot`, dont ceci est le pendant.
pub async fn consolidate_snapshot(
    pool: &Pool,
    legal_act_id: &ID,
    consolidation: LegalActReviewSnapshotConsolidation,
) -> Result<LegalActReviewSnapshot, StorageError> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        "INSERT INTO legal_act_review_snapshots (legal_act_id, snapshot, seq) VALUES ($1, $2, $3) \
         ON CONFLICT (legal_act_id) DO UPDATE \
         SET snapshot = excluded.snapshot, seq = excluded.seq, updated_at = now() \
         RETURNING *",
    )
    .bind(id::encode(legal_act_id))
    .bind(consolidation.snapshot)
    .bind(consolidation.seq)
    .fetch_one(&mut *tx)
    .await?;
    let consolidated = snapshot_from_row(row)?;

    sqlx::query("DELETE FROM legal_act_review_updates WHERE legal_act_id = $1 AND seq <= $2")
        .bind(id::encode(legal_act_id))
        .bind(consolidation.seq)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(consolidated)
}

/// Supprime tout l'historique CRDT (updates et snapshot) des commentaires
/// d'un acte légal.
pub async fn delete_legal_act_review_history(
    pool: &Pool,
    legal_act_id: &ID,
) -> Result<(), StorageError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM legal_act_review_updates WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM legal_act_review_snapshots WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
