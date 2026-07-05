/// Identifiant d'un acte légal.
#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct LegalActId(shared::id::ID);

impl LegalActId {
    pub fn new() -> Self {
        Self(shared::id::generate_id())
    }
}

impl Default for LegalActId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for LegalActId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type d'acte légal ICPE (arrêté préfectoral, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegalActKind {
    ArretePrefectoral,
    Autre(String),
}

/// Métadonnées d'un acte légal figé.
#[derive(Debug, Clone)]
pub struct LegalActMeta {
    pub id: LegalActId,
    pub kind: LegalActKind,
    pub autorite_id: Option<String>,
    /// Nom de l'autorité administrative émettrice (ex. « DREAL »), affiché
    /// dans le bloc-marque Marianne de l'en-tête ODT.
    pub authority_name: Option<String>,
    /// Nom de l'entité signataire de l'acte, affiché à droite de l'en-tête
    /// ODT, en vis-à-vis du bloc-marque Marianne.
    pub issuer_name: Option<String>,
    /// Date de signature au format ISO-8601 (`YYYY-MM-DD`).
    pub date_signature: Option<String>,
    /// Date d'entrée en vigueur au format ISO-8601 (`YYYY-MM-DD`).
    pub date_entree_en_vigueur: Option<String>,
    /// Identifiants des actes qui modifient cet acte.
    pub modifie_par: Vec<LegalActId>,
    /// Identifiants des actes modifiés par cet acte.
    pub modifie: Vec<LegalActId>,
}

impl LegalActMeta {
    pub fn new(kind: LegalActKind) -> Self {
        Self {
            id: LegalActId::new(),
            kind,
            autorite_id: None,
            authority_name: None,
            issuer_name: None,
            date_signature: None,
            date_entree_en_vigueur: None,
            modifie_par: vec![],
            modifie: vec![],
        }
    }
}

/// Trait de lecture pour un acte légal figé (lecture seule après finalisation).
pub trait LegalActRead {
    type Body: crate::traits::node::BodyRead;

    fn meta(&self) -> &LegalActMeta;
    fn title(&self) -> &str;
    fn body(&self) -> &Self::Body;
}

/// Trait d'écriture pour un acte légal en cours de rédaction.
pub trait LegalActWrite: LegalActRead {
    fn set_title(&mut self, title: &str);
    fn body_mut(&mut self) -> &mut Self::Body;
}

/// Identifiant d'un projet d'acte légal (acte en cours de rédaction).
#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct ProjectId(shared::id::ID);

impl ProjectId {
    pub fn new() -> Self {
        Self(shared::id::generate_id())
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Statut du workflow d'un projet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStatus {
    Redaction,
    Verification,
    Approbation,
    Finalise,
}

/// Métadonnées d'un projet d'acte.
#[derive(Debug, Clone)]
pub struct ProjectMeta {
    pub id: ProjectId,
    pub status: ProjectStatus,
    pub created_by: String,
}

impl ProjectMeta {
    pub fn new(created_by: impl Into<String>) -> Self {
        Self {
            id: ProjectId::new(),
            status: ProjectStatus::Redaction,
            created_by: created_by.into(),
        }
    }
}

/// Trait de lecture pour un projet d'acte légal.
/// Étend [`LegalActRead`] avec accès aux commentaires et notes de travail.
pub trait ProjectRead: LegalActRead {
    fn project_meta(&self) -> &ProjectMeta;
    fn is_editable(&self) -> bool {
        self.project_meta().status == ProjectStatus::Redaction
    }
}

/// Trait d'écriture pour un projet d'acte légal.
pub trait ProjectWrite: ProjectRead + LegalActWrite {
    fn set_status(&mut self, status: ProjectStatus);
}
