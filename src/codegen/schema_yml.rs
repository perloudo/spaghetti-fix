use serde::Serialize;

use crate::model::{Column, Layer, Model};

#[derive(Debug, Serialize)]
pub struct SchemaYml {
    pub version: i32,
    pub models: Vec<SchemaModel>,
}

#[derive(Debug, Serialize)]
pub struct SchemaModel {
    pub name: String,
    pub description: String,
    pub columns: Vec<SchemaColumn>,
}

#[derive(Debug, Serialize)]
pub struct SchemaColumn {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<String>,
}

pub fn generate_schema_yml(models: &[&Model]) -> String {
    let schema = SchemaYml {
        version: 2,
        models: models.iter().map(|m| model_to_schema(m)).collect(),
    };

    serde_yml::to_string(&schema).unwrap_or_default()
}

fn model_to_schema(model: &Model) -> SchemaModel {
    SchemaModel {
        name: model.model_name.clone(),
        description: format!(
            "Auto-generated {} model from CTE '{}'",
            model.layer, model.original_name
        ),
        columns: model.columns.iter().map(column_to_schema).collect(),
    }
}

fn column_to_schema(col: &Column) -> SchemaColumn {
    let tests = infer_tests(&col.name);
    SchemaColumn {
        name: col.name.clone(),
        description: String::new(),
        tests,
    }
}

fn infer_tests(column_name: &str) -> Vec<String> {
    let name = column_name.to_lowercase();
    let mut tests = Vec::new();

    if name.ends_with("_id") || name == "id" {
        tests.push("unique".to_string());
        tests.push("not_null".to_string());
    } else if name.ends_with("_at") || name.ends_with("_date") || name.ends_with("_timestamp") {
        tests.push("not_null".to_string());
    }

    tests
}

pub fn schema_filename(layer: &Layer) -> String {
    format!("_{}__schema.yml", layer.dir_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_id_tests() {
        let tests = infer_tests("user_id");
        assert_eq!(tests, vec!["unique", "not_null"]);
    }

    #[test]
    fn test_infer_bare_id() {
        let tests = infer_tests("id");
        assert_eq!(tests, vec!["unique", "not_null"]);
    }

    #[test]
    fn test_infer_date_tests() {
        assert_eq!(infer_tests("created_at"), vec!["not_null"]);
        assert_eq!(infer_tests("order_date"), vec!["not_null"]);
        assert_eq!(infer_tests("updated_timestamp"), vec!["not_null"]);
    }

    #[test]
    fn test_infer_no_tests() {
        assert!(infer_tests("user_name").is_empty());
        assert!(infer_tests("amount").is_empty());
    }

    #[test]
    fn test_schema_filename() {
        assert_eq!(schema_filename(&Layer::Staging), "_staging__schema.yml");
        assert_eq!(
            schema_filename(&Layer::Intermediate),
            "_intermediate__schema.yml"
        );
        assert_eq!(schema_filename(&Layer::Marts), "_marts__schema.yml");
    }
}
