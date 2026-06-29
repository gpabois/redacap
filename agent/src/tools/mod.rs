//! Implémentations concrètes des outils listés dans la documentation de
//! l'agent IA : chaque outil est un [`crate::Tool`] indépendant, à
//! enregistrer dans un [`crate::ToolRegistry`].

mod georisques;
mod interaction;
mod legal_act_editor;
mod legifrance;
mod metadata;

pub use georisques::{GeorisquesClient, GeorisquesConfig, GeorisquesQueryTool, IcpeQueryTool};
pub use interaction::{AskUserTool, RequestDocumentTool};
pub use legal_act_editor::{FillSectionTool, GenerateNumberingTool, ValidateStructureTool};
pub use legifrance::{LegifranceClient, LegifranceConfig, LegifranceFetchTool, LegifranceSearchTool};
pub use metadata::{ReadMetadataTool, WriteMetadataTool};
