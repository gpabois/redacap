use thiserror::Error;

/// Erreurs survenant lors de l'échange avec un modèle de langage.
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("échec de la requête vers le modèle de langage : {0}")]
    Request(#[from] reqwest::Error),
    #[error("le modèle de langage a renvoyé une réponse invalide : {0}")]
    InvalidResponse(String),
}

/// Erreurs survenant lors de l'exécution d'un outil par l'agent.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("échec de la requête HTTP : {0}")]
    Http(#[from] reqwest::Error),
    #[error("arguments d'outil invalides : {0}")]
    InvalidArguments(String),
    #[error("l'utilisateur a refusé l'exécution de l'outil")]
    Rejected,
    #[error("{0}")]
    Other(String),
}

/// Erreurs survenant pendant l'exécution de la boucle agentique.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("échec de l'outil « {name} » : {source}")]
    Tool {
        name: String,
        #[source]
        source: ToolError,
    },
    #[error("nombre maximal d'itérations atteint ({0}) sans réponse finale du modèle")]
    MaxStepsExceeded(u32),
    #[error("aucune orchestration en attente d'une réponse humaine à reprendre")]
    NotPaused,
    #[error("la réponse fournie ne correspond pas à la question en attente")]
    MismatchedAnswer,
}
