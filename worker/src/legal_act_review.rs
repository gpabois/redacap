//! Consolidation périodique des snapshots Yrs des commentaires/notes de
//! travail (`legal_act::Review`) des actes légaux, en tout point symétrique
//! de [`super::legal_act`] pour le corps de l'acte (voir `storage::
//! legal_act_review`), mais sur son propre journal/instantané : les deux
//! documents Yrs sont consolidés indépendamment.

use shared::id::ID;
use shared::model::LegalActReviewSnapshotConsolidation;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

/// Consolide le snapshot de commentaires de chaque acte légal ayant des
/// mises à jour en attente. Une erreur sur un acte donné est journalisée et
/// n'interrompt pas le traitement des autres.
pub async fn consolidate_pending(pool: &storage::Pool) -> anyhow::Result<()> {
    let legal_act_ids =
        storage::legal_act_review::list_legal_acts_with_pending_updates(pool).await?;
    for legal_act_id in legal_act_ids {
        if let Err(error) = consolidate_one(pool, &legal_act_id).await {
            eprintln!(
                "échec de la consolidation du snapshot de commentaires de {legal_act_id} : {error}"
            );
        }
    }
    Ok(())
}

/// Pendant de `super::legal_act::consolidate_one` pour le document de commentaires.
async fn consolidate_one(pool: &storage::Pool, legal_act_id: &ID) -> anyhow::Result<()> {
    let snapshot = storage::legal_act_review::get_snapshot(pool, legal_act_id).await?;
    let since_seq = snapshot.as_ref().map_or(0, |snapshot| snapshot.seq);
    let updates =
        storage::legal_act_review::list_updates_since(pool, legal_act_id, since_seq).await?;
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

    storage::legal_act_review::consolidate_snapshot(
        pool,
        legal_act_id,
        LegalActReviewSnapshotConsolidation {
            snapshot: merged,
            seq: last_seq,
        },
    )
    .await?;
    Ok(())
}
