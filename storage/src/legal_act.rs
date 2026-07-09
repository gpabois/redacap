use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{
    CreateLegalAct, CreateLegalActUpdate, LegalAct, LegalActSnapshot,
    LegalActSnapshotConsolidation, LegalActStatus, LegalActUpdate,
};

fn legal_act_from_row(row: PgRow) -> Result<LegalAct, StorageError> {
    let status: String = row.try_get("status")?;
    Ok(LegalAct {
        id: id::column(&row, "id")?,
        title: row.try_get("title")?,
        domain_id: id::column(&row, "domain_id")?,
        authority_id: id::column(&row, "authority_id")?,
        status: status.parse().map_err(StorageError::InvalidId)?,
        created_by: id::column(&row, "created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Crée un projet d'acte légal, en statut initial `Redaction`.
pub async fn create_legal_act(pool: &Pool, args: CreateLegalAct) -> Result<LegalAct, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO legal_acts (id, title, domain_id, authority_id, status, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(args.title)
    .bind(id::encode(&args.domain_id))
    .bind(id::encode(&args.authority_id))
    .bind(LegalActStatus::Redaction.to_string())
    .bind(id::encode(&args.created_by))
    .fetch_one(pool)
    .await?;
    legal_act_from_row(row)
}

/// Récupère un projet d'acte légal par son identifiant.
pub async fn get_legal_act(pool: &Pool, legal_act_id: &ID) -> Result<LegalAct, StorageError> {
    let row = sqlx::query("SELECT * FROM legal_acts WHERE id = $1")
        .bind(id::encode(legal_act_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    legal_act_from_row(row)
}

/// Liste l'ensemble des projets d'acte légal, tous statuts confondus —
/// utilisé par le panneau administrateur pour l'attribution de permissions
/// (voir `app::pages::admin::permissions`), qui doit pouvoir cibler
/// n'importe quel acte indépendamment des droits de l'administrateur courant.
pub async fn list_all_legal_acts(pool: &Pool) -> Result<Vec<LegalAct>, StorageError> {
    let rows = sqlx::query("SELECT * FROM legal_acts ORDER BY updated_at DESC")
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(legal_act_from_row).collect()
}

/// Liste les projets d'acte légal visibles par un utilisateur pour le
/// tableau de bord `/` : ceux dont il est l'auteur, plus ceux listés dans
/// `accessible_ids` (droits directs ou hérités de ses groupes, résolus en
/// amont par `app::auth::accessible_legal_act_ids`), triés du plus
/// récemment modifié au plus ancien.
pub async fn list_legal_acts_for_user(
    pool: &Pool,
    user_id: &ID,
    accessible_ids: &[ID],
) -> Result<Vec<LegalAct>, StorageError> {
    let accessible_ids: Vec<Vec<u8>> = accessible_ids
        .iter()
        .map(|id| id::encode(id).to_vec())
        .collect();
    let rows = sqlx::query(
        "SELECT * FROM legal_acts WHERE created_by = $1 OR id = ANY($2) \
         ORDER BY updated_at DESC",
    )
    .bind(id::encode(user_id))
    .bind(accessible_ids)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(legal_act_from_row).collect()
}

fn update_from_row(row: PgRow) -> Result<LegalActUpdate, StorageError> {
    Ok(LegalActUpdate {
        legal_act_id: id::column(&row, "legal_act_id")?,
        seq: row.try_get("seq")?,
        update: row.try_get("update")?,
        author_id: id::column(&row, "author_id")?,
        created_at: row.try_get("created_at")?,
    })
}

fn snapshot_from_row(row: PgRow) -> Result<LegalActSnapshot, StorageError> {
    Ok(LegalActSnapshot {
        legal_act_id: id::column(&row, "legal_act_id")?,
        snapshot: row.try_get("snapshot")?,
        seq: row.try_get("seq")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Journalise une mise à jour Yrs incrémentale (append-only, aucune transformation).
pub async fn append_update(
    pool: &Pool,
    args: CreateLegalActUpdate,
) -> Result<LegalActUpdate, StorageError> {
    let row = sqlx::query(
        "INSERT INTO legal_act_updates (legal_act_id, seq, update, author_id) \
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
) -> Result<Vec<LegalActUpdate>, StorageError> {
    let rows = sqlx::query(
        "SELECT * FROM legal_act_updates WHERE legal_act_id = $1 AND seq > $2 ORDER BY seq",
    )
    .bind(id::encode(legal_act_id))
    .bind(since_seq)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(update_from_row).collect()
}

/// Récupère le dernier instantané consolidé d'un acte légal, s'il existe.
pub async fn get_snapshot(
    pool: &Pool,
    legal_act_id: &ID,
) -> Result<Option<LegalActSnapshot>, StorageError> {
    let row = sqlx::query("SELECT * FROM legal_act_snapshots WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .fetch_optional(pool)
        .await?;
    row.map(snapshot_from_row).transpose()
}

/// Identifiants des actes légaux ayant au moins une mise à jour Yrs en
/// attente de consolidation (voir [`consolidate_snapshot`]) — utilisé par
/// `worker` pour cibler sa tâche périodique de consolidation.
pub async fn list_legal_acts_with_pending_updates(pool: &Pool) -> Result<Vec<ID>, StorageError> {
    let rows = sqlx::query("SELECT DISTINCT legal_act_id FROM legal_act_updates")
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| id::column(row, "legal_act_id"))
        .collect()
}

/// Consolide un instantané et purge le journal d'updates devenu redondant, atomiquement.
///
/// Respecte l'invariant : le snapshot est écrit (créé ou remplacé) avant que les
/// updates de séquence `<= seq` ne soient supprimées, dans une même transaction.
pub async fn consolidate_snapshot(
    pool: &Pool,
    legal_act_id: &ID,
    consolidation: LegalActSnapshotConsolidation,
) -> Result<LegalActSnapshot, StorageError> {
    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        "INSERT INTO legal_act_snapshots (legal_act_id, snapshot, seq) VALUES ($1, $2, $3) \
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

    sqlx::query("DELETE FROM legal_act_updates WHERE legal_act_id = $1 AND seq <= $2")
        .bind(id::encode(legal_act_id))
        .bind(consolidation.seq)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(consolidated)
}

/// Supprime tout l'historique CRDT (updates et snapshot) d'un acte légal.
pub async fn delete_legal_act_history(pool: &Pool, legal_act_id: &ID) -> Result<(), StorageError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM legal_act_updates WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM legal_act_snapshots WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Supprime définitivement un projet d'acte légal : son historique CRDT
/// (corps et commentaires — aucune contrainte de clé étrangère sur ces
/// tables, donc pas de suppression automatique par cascade), les permissions
/// accordées spécifiquement sur ce projet (`permissions.resource_id` est une
/// colonne polymorphe, également sans contrainte de clé étrangère), puis le
/// projet lui-même. Les intentions rattachées (`legal_act_intentions`) sont
/// supprimées automatiquement par la contrainte `ON DELETE CASCADE` de cette
/// table. Le journal d'audit n'est volontairement pas purgé (`audit_log`
/// doit survivre à la suppression de la ressource qu'il documente).
///
/// Tout est fait dans une seule transaction : soit tout disparaît, soit rien.
pub async fn delete_legal_act(pool: &Pool, legal_act_id: &ID) -> Result<(), StorageError> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM legal_act_updates WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM legal_act_snapshots WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM legal_act_review_updates WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM legal_act_review_snapshots WHERE legal_act_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM permissions WHERE resource_type = 'legal_act' AND resource_id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;

    let result = sqlx::query("DELETE FROM legal_acts WHERE id = $1")
        .bind(id::encode(legal_act_id))
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound);
    }

    tx.commit().await?;
    Ok(())
}
