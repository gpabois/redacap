//! Persistance des documents externes fournis pour un projet d'acte légal
//! (voir migration `0020_legal_act_documents`) : alimentée aussi bien par
//! l'inspecteur (panneau « Fichiers » de l'éditeur, voir
//! `app::pages::project_documents`) que par l'agent IA (outil
//! `request_document`, voir `agent::tools::interaction`), et lue par les
//! outils `read_document`/`search_documents`/`fetch_document_by_url` (voir
//! `agent::tools::document`).

use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::db::Pool;
use crate::error::StorageError;
use crate::id;
use shared::id::ID;
use shared::model::{LegalActDocument, LegalActDocumentSummary};

fn from_row(row: PgRow) -> Result<LegalActDocument, StorageError> {
    Ok(LegalActDocument {
        id: id::column(&row, "id")?,
        legal_act_id: id::column(&row, "legal_act_id")?,
        file_name: row.try_get("file_name")?,
        mime_type: row.try_get("mime_type")?,
        bytes: row.try_get("bytes")?,
        label: row.try_get("label")?,
        uploaded_by: id::column(&row, "uploaded_by")?,
        created_at: row.try_get("created_at")?,
    })
}

fn summary_from_row(row: PgRow) -> Result<LegalActDocumentSummary, StorageError> {
    Ok(LegalActDocumentSummary {
        id: id::column(&row, "id")?,
        legal_act_id: id::column(&row, "legal_act_id")?,
        file_name: row.try_get("file_name")?,
        mime_type: row.try_get("mime_type")?,
        size: row.try_get("size")?,
        label: row.try_get("label")?,
        uploaded_by: id::column(&row, "uploaded_by")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Enregistre un document fourni pour le projet `legal_act_id`, par
/// l'inspecteur `uploaded_by` (soit directement depuis le panneau
/// « Fichiers », soit en réponse à une pause `request_document` de l'agent) :
/// `label` est le libellé sémantique du document (voir
/// [`LegalActDocument::label`]), chaîne vide si aucun n'est fourni.
pub async fn store_document(
    pool: &Pool,
    legal_act_id: &ID,
    file_name: &str,
    mime_type: &str,
    bytes: Vec<u8>,
    label: &str,
    uploaded_by: &ID,
) -> Result<LegalActDocument, StorageError> {
    let new_id = shared::id::generate_id();
    let row = sqlx::query(
        "INSERT INTO legal_act_documents (id, legal_act_id, file_name, mime_type, bytes, label, uploaded_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
    )
    .bind(id::encode(&new_id))
    .bind(id::encode(legal_act_id))
    .bind(file_name)
    .bind(mime_type)
    .bind(bytes)
    .bind(label)
    .bind(id::encode(uploaded_by))
    .fetch_one(pool)
    .await?;
    from_row(row)
}

/// Relit un document précédemment stocké via [`store_document`], contenu
/// binaire compris, pour l'outil `read_document` de l'agent.
pub async fn fetch_document(pool: &Pool, document_id: &ID) -> Result<LegalActDocument, StorageError> {
    let row = sqlx::query("SELECT * FROM legal_act_documents WHERE id = $1")
        .bind(id::encode(document_id))
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound)?;
    from_row(row)
}

/// Liste les documents du projet `legal_act_id`, les plus récents en
/// dernier, sans leur contenu binaire (voir [`LegalActDocumentSummary`]) :
/// pour le panneau « Fichiers » et l'outil `search_documents`.
pub async fn list_documents_for_legal_act(
    pool: &Pool,
    legal_act_id: &ID,
) -> Result<Vec<LegalActDocumentSummary>, StorageError> {
    let rows = sqlx::query(
        "SELECT id, legal_act_id, file_name, mime_type, octet_length(bytes)::bigint AS size, \
         label, uploaded_by, created_at FROM legal_act_documents WHERE legal_act_id = $1 \
         ORDER BY created_at ASC",
    )
    .bind(id::encode(legal_act_id))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(summary_from_row).collect()
}

/// Supprime le document `document_id` du projet `legal_act_id` et renvoie
/// son nom de fichier (pour l'événement diffusé aux pairs de la salle, voir
/// `shared::broadcast::DocumentsChangedEvent`) : le filtre sur
/// `legal_act_id` empêche de supprimer un document d'un autre projet en
/// falsifiant simplement l'identifiant depuis le panneau « Fichiers ».
pub async fn delete_document(
    pool: &Pool,
    legal_act_id: &ID,
    document_id: &ID,
) -> Result<String, StorageError> {
    let row = sqlx::query(
        "DELETE FROM legal_act_documents WHERE id = $1 AND legal_act_id = $2 \
         RETURNING file_name",
    )
    .bind(id::encode(document_id))
    .bind(id::encode(legal_act_id))
    .fetch_optional(pool)
    .await?
    .ok_or(StorageError::NotFound)?;
    Ok(row.try_get("file_name")?)
}
