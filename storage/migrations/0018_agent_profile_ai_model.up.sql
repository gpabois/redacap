-- Permet à chaque profil d'agent expert d'être exécuté par un modèle IA
-- spécifique plutôt que par le modèle actif par défaut (voir
-- agent::orchestration::Orchestrator), pour tirer parti des forces propres
-- à chaque modèle selon la tâche déléguée. NULL (par défaut) conserve
-- l'ancien comportement : le modèle actif de /admin/ai-models.
ALTER TABLE agent_profiles
    ADD COLUMN ai_model_id BYTEA REFERENCES ai_models (id) ON DELETE SET NULL;
