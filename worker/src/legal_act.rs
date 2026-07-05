//! Consolidation périodique des snapshots Yrs des actes légaux : fusionne
//! le dernier instantané persisté avec les mises à jour incrémentales
//! accumulées depuis (voir `storage::CLAUDE.md` § Actes légaux — CRDT Yrs),
//! et purge le journal d'updates devenu redondant. Alternative différée à
//! une consolidation au fil de l'édition : le serveur journalise chaque
//! mise à jour au fil de l'eau (voir `server::editor::state::EditorRoom::
//! record_and_broadcast`), et laisse ce worker en compacter l'historique à
//! intervalle régulier (voir [`super::run`]).

use shared::id::ID;
use shared::model::LegalActSnapshotConsolidation;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

/// Consolide le snapshot de chaque acte légal ayant des mises à jour en
/// attente. Une erreur sur un acte donné est journalisée et n'interrompt
/// pas le traitement des autres.
pub async fn consolidate_pending(pool: &storage::Pool) -> anyhow::Result<()> {
    let legal_act_ids = storage::legal_act::list_legal_acts_with_pending_updates(pool).await?;
    for legal_act_id in legal_act_ids {
        if let Err(error) = consolidate_one(pool, &legal_act_id).await {
            eprintln!("échec de la consolidation du snapshot de {legal_act_id} : {error}");
        }
    }
    Ok(())
}

/// Fusionne le dernier snapshot de `legal_act_id` (s'il existe) avec les
/// mises à jour postérieures, et remplace le snapshot par cet état fusionné
/// (voir `storage::legal_act::consolidate_snapshot`). N'a aucun effet si
/// aucune mise à jour n'est postérieure au dernier snapshot.
async fn consolidate_one(pool: &storage::Pool, legal_act_id: &ID) -> anyhow::Result<()> {
    let snapshot = storage::legal_act::get_snapshot(pool, legal_act_id).await?;
    let since_seq = snapshot.as_ref().map_or(0, |snapshot| snapshot.seq);
    let updates = storage::legal_act::list_updates_since(pool, legal_act_id, since_seq).await?;
    let Some(last_seq) = updates.last().map(|update| update.seq) else {
        return Ok(());
    };

    let doc = Doc::new();
    if let Some(snapshot) = &snapshot {
        let update = Update::decode_v1(&snapshot.snapshot)
            .map_err(|error| anyhow::anyhow!("instantané corrompu : {error}"))?;
        doc.transact_mut()
            .apply_update(update)
            .map_err(|error| anyhow::anyhow!("échec d'application de l'instantané : {error}"))?;
    }
    {
        let mut txn = doc.transact_mut();
        for update in &updates {
            let decoded = Update::decode_v1(&update.update).map_err(|error| {
                anyhow::anyhow!("mise à jour corrompue (seq {}) : {error}", update.seq)
            })?;
            txn.apply_update(decoded).map_err(|error| {
                anyhow::anyhow!(
                    "échec d'application de la mise à jour (seq {}) : {error}",
                    update.seq
                )
            })?;
        }
    }
    let merged = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());

    storage::legal_act::consolidate_snapshot(
        pool,
        legal_act_id,
        LegalActSnapshotConsolidation {
            snapshot: merged,
            seq: last_seq,
        },
    )
    .await?;
    Ok(())
}
