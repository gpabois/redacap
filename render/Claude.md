## Génération de documents (`render`)

- **ODT** : format de sortie officiel, généré via des templates ODT (bibliothèque `lopdf` ou équivalent Rust pour la structure XML ODF).
- **PDF** : rendu via conversion ODT→PDF (LibreOffice headless en subprocess, ou bibliothèque Rust native).
- Les fonctions de rendu sont **pures** : `fn render_odt(act: &LegalAct) -> Result<Vec<u8>, RenderError>`.
- Aucun I/O dans le crate `render` : les appels réseau/filesystem se font dans `server`.
