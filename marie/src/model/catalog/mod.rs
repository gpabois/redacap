use std::collections::HashMap;

use crate::model::declaration::ModelDeclaration;

#[derive(Default, Clone)]
pub struct ModelCatalog(HashMap<String, ModelDeclaration>);
