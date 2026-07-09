-- Libellé sémantique d'un document (ex. « rapport d'inspection ICPE du
-- 12/03/2024 »), distinct de `file_name` : permet à l'agent IA de retrouver
-- un document par ce qu'il représente plutôt que par le nom de fichier brut,
-- souvent opaque (`scan003.pdf`). Chaîne vide si aucun libellé n'a été
-- fourni (aucune rétro-normalisation des documents existants).
ALTER TABLE legal_act_documents ADD COLUMN label TEXT NOT NULL DEFAULT '';
