use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum Layer {
    Staging,
    Intermediate,
    Marts,
}

impl Layer {
    pub fn dir_name(&self) -> &str {
        match self {
            Layer::Staging => "staging",
            Layer::Intermediate => "intermediate",
            Layer::Marts => "marts",
        }
    }

    pub fn prefix(&self) -> &str {
        match self {
            Layer::Staging => "stg_",
            Layer::Intermediate => "int_",
            Layer::Marts => "",
        }
    }
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.dir_name())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Column {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Model {
    pub original_name: String,
    pub model_name: String,
    pub layer: Layer,
    pub sql_body: String,
    pub columns: Vec<Column>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExtractedCte {
    pub name: String,
    pub body: String,
    pub table_refs: HashSet<String>,
    pub columns: Vec<Column>,
    pub is_recursive: bool,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct DbtProject {
    pub name: String,
    pub models: Vec<Model>,
    pub final_select: Option<FinalSelect>,
}

#[derive(Debug, Clone)]
pub struct FinalSelect {
    pub body: String,
    pub table_refs: HashSet<String>,
    pub columns: Vec<Column>,
}
