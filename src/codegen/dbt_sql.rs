use std::collections::HashMap;

use regex::Regex;

use crate::model::Model;

/// Generate a dbt SQL model body by replacing CTE references with ref() calls.
/// Uses regex replacement: `FROM|JOIN <cte_name>` → `FROM|JOIN {{ ref('<model_name>') }}`
pub fn generate_model_sql(model: &Model, name_to_model: &HashMap<String, String>) -> String {
    let mut sql = model.sql_body.clone();

    for (original_cte_name, target_model_name) in name_to_model {
        // Match FROM/JOIN followed by the CTE name (case-insensitive, word boundary)
        let pattern = format!(
            r"(?i)\b(FROM|JOIN)\s+{}(?:\b|$)",
            regex::escape(original_cte_name)
        );
        let re = Regex::new(&pattern).unwrap();
        let replacement = format!("${{1}} {{{{ ref('{}') }}}}", target_model_name);
        sql = re.replace_all(&sql, replacement.as_str()).to_string();
    }

    sql
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, Model};

    #[test]
    fn test_ref_substitution_from() {
        let model = Model {
            original_name: "cleaned".to_string(),
            model_name: "int_cleaned".to_string(),
            layer: Layer::Intermediate,
            sql_body: "SELECT id, name FROM source WHERE id > 0".to_string(),
            columns: Vec::new(),
            dependencies: vec!["source".to_string()],
        };
        let mut map = HashMap::new();
        map.insert("source".to_string(), "stg_source".to_string());

        let result = generate_model_sql(&model, &map);
        assert!(result.contains("{{ ref('stg_source') }}"));
        assert!(!result.contains("FROM source"));
    }

    #[test]
    fn test_ref_substitution_join() {
        let model = Model {
            original_name: "combined".to_string(),
            model_name: "combined".to_string(),
            layer: Layer::Marts,
            sql_body: "SELECT a.id FROM orders JOIN users ON orders.user_id = users.id".to_string(),
            columns: Vec::new(),
            dependencies: vec!["orders".to_string(), "users".to_string()],
        };
        let mut map = HashMap::new();
        map.insert("orders".to_string(), "stg_orders".to_string());
        map.insert("users".to_string(), "stg_users".to_string());

        let result = generate_model_sql(&model, &map);
        assert!(result.contains("{{ ref('stg_orders') }}"));
        assert!(result.contains("{{ ref('stg_users') }}"));
    }

    #[test]
    fn test_ref_no_substring_collision() {
        // CTE "orders" should NOT match "new_orders" (word boundary validation)
        let model = Model {
            original_name: "summary".to_string(),
            model_name: "summary".to_string(),
            layer: Layer::Marts,
            sql_body: "SELECT * FROM new_orders JOIN orders ON new_orders.id = orders.id"
                .to_string(),
            columns: Vec::new(),
            dependencies: vec!["orders".to_string()],
        };
        let mut map = HashMap::new();
        map.insert("orders".to_string(), "stg_orders".to_string());

        let result = generate_model_sql(&model, &map);
        // "orders" should be replaced
        assert!(result.contains("{{ ref('stg_orders') }}"));
        // "new_orders" should NOT be replaced (not in the map, and word boundary protects it)
        assert!(
            result.contains("new_orders"),
            "new_orders should remain unchanged, got: {}",
            result
        );
    }

    #[test]
    fn test_ref_case_insensitive() {
        // FROM Orders (uppercase O) should match CTE "orders" via (?i) flag
        let model = Model {
            original_name: "report".to_string(),
            model_name: "report".to_string(),
            layer: Layer::Marts,
            sql_body: "SELECT * FROM Orders WHERE amount > 0".to_string(),
            columns: Vec::new(),
            dependencies: vec!["orders".to_string()],
        };
        let mut map = HashMap::new();
        map.insert("orders".to_string(), "stg_orders".to_string());

        let result = generate_model_sql(&model, &map);
        assert!(
            result.contains("{{ ref('stg_orders') }}"),
            "Case-insensitive match should work, got: {}",
            result
        );
        assert!(
            !result.contains("FROM Orders"),
            "Original cased name should be replaced, got: {}",
            result
        );
    }
}
