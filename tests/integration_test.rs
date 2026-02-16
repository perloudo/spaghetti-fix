use std::collections::HashMap;
use std::path::PathBuf;

use spaghetti_fix::codegen::{dbt_sql, dot, schema_yml};
use spaghetti_fix::graph::DependencyGraph;
use spaghetti_fix::layering;
use spaghetti_fix::model::{Layer, Model};
use spaghetti_fix::parser;

use sqlparser::dialect::AnsiDialect;
use tempfile::TempDir;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run_pipeline(fixture: &str) -> (Vec<Model>, HashMap<String, String>) {
    let dialect = AnsiDialect {};
    let stmts = parser::parse_sql_file(&fixture_path(fixture), &dialect).unwrap();
    let (ctes, final_select) = parser::extract_ctes(&stmts).unwrap();

    let regular: Vec<_> = ctes.into_iter().filter(|c| !c.is_recursive).collect();
    let dep_graph = DependencyGraph::from_ctes(&regular).unwrap();
    let topo_order = dep_graph.topological_order().unwrap();

    let mut models = Vec::new();
    let mut name_to_model = HashMap::new();

    for cte_name in &topo_order {
        let cte = regular.iter().find(|c| &c.name == cte_name).unwrap();
        let layer = layering::classify_layer(cte_name, &dep_graph);
        let model_name = layering::model_name(cte_name, &layer);
        name_to_model.insert(cte_name.clone(), model_name.clone());

        models.push(Model {
            original_name: cte.name.clone(),
            model_name,
            layer,
            sql_body: cte.body.clone(),
            columns: cte.columns.clone(),
            dependencies: cte.table_refs.iter().cloned().collect(),
        });
    }

    if let Some(fs) = &final_select {
        let final_name = "test_final".to_string();
        models.push(Model {
            original_name: final_name.clone(),
            model_name: final_name.clone(),
            layer: Layer::Marts,
            sql_body: fs.body.clone(),
            columns: fs.columns.clone(),
            dependencies: fs.table_refs.iter().cloned().collect(),
        });
        name_to_model.insert(final_name.clone(), final_name);
    }

    (models, name_to_model)
}

#[test]
fn test_simple_ctes_end_to_end() {
    let (models, name_to_model) = run_pipeline("simple_ctes.sql");

    // Should have 3 CTEs + 1 final = 4 models
    assert_eq!(models.len(), 4);

    // Check layering
    let staging: Vec<_> = models
        .iter()
        .filter(|m| m.layer == Layer::Staging)
        .collect();
    let intermediate: Vec<_> = models
        .iter()
        .filter(|m| m.layer == Layer::Intermediate)
        .collect();
    let marts: Vec<_> = models.iter().filter(|m| m.layer == Layer::Marts).collect();

    assert_eq!(staging.len(), 1);
    assert_eq!(staging[0].model_name, "stg_source");

    assert_eq!(intermediate.len(), 1);
    assert_eq!(intermediate[0].model_name, "int_cleaned");

    assert_eq!(marts.len(), 2); // final_output + test_final

    // Check ref() substitution
    let cleaned_sql = dbt_sql::generate_model_sql(&models[1], &name_to_model);
    assert!(
        cleaned_sql.contains("{{ ref('stg_source') }}"),
        "Expected ref() substitution, got: {}",
        cleaned_sql
    );
    assert!(
        !cleaned_sql.contains("FROM source"),
        "Raw CTE name should be replaced"
    );
}

#[test]
fn test_diamond_ctes_end_to_end() {
    let (models, name_to_model) = run_pipeline("diamond_ctes.sql");

    // 4 CTEs + 1 final = 5
    assert_eq!(models.len(), 5);

    // base_data should be staging (no CTE deps)
    let base = models
        .iter()
        .find(|m| m.original_name == "base_data")
        .unwrap();
    assert_eq!(base.layer, Layer::Staging);

    // orders and customers are intermediate
    let orders = models.iter().find(|m| m.original_name == "orders").unwrap();
    assert_eq!(orders.layer, Layer::Intermediate);

    let customers = models
        .iter()
        .find(|m| m.original_name == "customers")
        .unwrap();
    assert_eq!(customers.layer, Layer::Intermediate);

    // combined is marts (terminal CTE node — it has no CTE dependents except final)
    let combined = models
        .iter()
        .find(|m| m.original_name == "combined")
        .unwrap();
    assert_eq!(combined.layer, Layer::Marts);

    // Check diamond ref() wiring
    let combined_sql = dbt_sql::generate_model_sql(combined, &name_to_model);
    assert!(combined_sql.contains("{{ ref('int_orders') }}"));
    assert!(combined_sql.contains("{{ ref('int_customers') }}"));
}

#[test]
fn test_schema_yml_generation() {
    let (models, _) = run_pipeline("simple_ctes.sql");

    let staging_models: Vec<&Model> = models
        .iter()
        .filter(|m| m.layer == Layer::Staging)
        .collect();
    let schema = schema_yml::generate_schema_yml(&staging_models);

    assert!(schema.contains("version: 2"));
    assert!(schema.contains("stg_source"));
    // id column should have unique + not_null tests
    assert!(schema.contains("unique"));
    assert!(schema.contains("not_null"));
    // created_at should have not_null
    assert!(schema.contains("created_at"));
}

#[test]
fn test_dot_lineage_generation() {
    let (models, _) = run_pipeline("diamond_ctes.sql");

    let dot_output = dot::generate_dot(&models);
    assert!(dot_output.contains("digraph lineage"));
    assert!(dot_output.contains("stg_base_data"));
    assert!(dot_output.contains("int_orders"));
    assert!(dot_output.contains("int_customers"));
    assert!(dot_output.contains("combined"));
    // Check edges exist
    assert!(dot_output.contains("\"stg_base_data\" -> \"int_orders\""));
    assert!(dot_output.contains("\"stg_base_data\" -> \"int_customers\""));
}

#[test]
fn test_real_world_acceptance() {
    let (models, name_to_model) = run_pipeline("real_world.sql");

    // Should have 20 CTEs + 1 final = 21 models
    assert_eq!(models.len(), 21);

    // Check we have all three layers populated
    let staging_count = models.iter().filter(|m| m.layer == Layer::Staging).count();
    let int_count = models
        .iter()
        .filter(|m| m.layer == Layer::Intermediate)
        .count();
    let marts_count = models.iter().filter(|m| m.layer == Layer::Marts).count();

    assert!(staging_count > 0, "Should have staging models");
    assert!(int_count > 0, "Should have intermediate models");
    assert!(marts_count > 0, "Should have marts models");

    // Check that ref() substitution works across the board
    for model in &models {
        let sql = dbt_sql::generate_model_sql(model, &name_to_model);
        // Any model with dependencies should have ref() calls
        for dep in &model.dependencies {
            if let Some(dep_model_name) = name_to_model.get(dep) {
                let ref_call = format!("ref('{}')", dep_model_name);
                assert!(
                    sql.contains(&ref_call),
                    "Model '{}' should reference '{}' via ref(), got:\n{}",
                    model.model_name,
                    dep_model_name,
                    sql
                );
            }
        }
    }

    // Check DOT generation doesn't crash
    let dot_output = dot::generate_dot(&models);
    assert!(dot_output.contains("digraph lineage"));
    assert!(
        dot_output.lines().count() > 20,
        "DOT should have many lines for 21 models"
    );
}

#[test]
fn test_file_output_structure() {
    let (models, name_to_model) = run_pipeline("simple_ctes.sql");

    let tmp = TempDir::new().unwrap();
    let output_dir = tmp.path();
    let models_dir = output_dir.join("models");

    // Create directories
    for layer in &[Layer::Staging, Layer::Intermediate, Layer::Marts] {
        std::fs::create_dir_all(models_dir.join(layer.dir_name())).unwrap();
    }

    // Write SQL files
    for model in &models {
        let sql = dbt_sql::generate_model_sql(model, &name_to_model);
        let path = models_dir
            .join(model.layer.dir_name())
            .join(format!("{}.sql", model.model_name));
        std::fs::write(&path, &sql).unwrap();
    }

    // Write schema files
    for layer in &[Layer::Staging, Layer::Intermediate, Layer::Marts] {
        let layer_models: Vec<&Model> = models.iter().filter(|m| &m.layer == layer).collect();
        if !layer_models.is_empty() {
            let schema = schema_yml::generate_schema_yml(&layer_models);
            let path = models_dir
                .join(layer.dir_name())
                .join(schema_yml::schema_filename(layer));
            std::fs::write(&path, &schema).unwrap();
        }
    }

    // Write DOT
    let dot_content = dot::generate_dot(&models);
    std::fs::write(output_dir.join("lineage.dot"), &dot_content).unwrap();

    // Verify files exist
    assert!(models_dir.join("staging/stg_source.sql").exists());
    assert!(models_dir.join("intermediate/int_cleaned.sql").exists());
    assert!(models_dir.join("marts/final_output.sql").exists());
    assert!(models_dir.join("staging/_staging__schema.yml").exists());
    assert!(output_dir.join("lineage.dot").exists());

    // Verify SQL content
    let content = std::fs::read_to_string(models_dir.join("intermediate/int_cleaned.sql")).unwrap();
    assert!(content.contains("{{ ref('stg_source') }}"));

    // Verify schema content
    let schema = std::fs::read_to_string(models_dir.join("staging/_staging__schema.yml")).unwrap();
    assert!(schema.contains("version: 2"));
    assert!(schema.contains("stg_source"));
}

// --- New integration tests ---

#[test]
fn test_window_functions_end_to_end() {
    let (models, name_to_model) = run_pipeline("window_functions.sql");

    // 3 CTEs + 1 final = 4 models
    assert_eq!(models.len(), 4);

    // base_events = staging, ranked_events = intermediate, latest_per_user = marts
    let base = models
        .iter()
        .find(|m| m.original_name == "base_events")
        .unwrap();
    assert_eq!(base.layer, Layer::Staging);

    let ranked = models
        .iter()
        .find(|m| m.original_name == "ranked_events")
        .unwrap();
    assert_eq!(ranked.layer, Layer::Intermediate);

    let latest = models
        .iter()
        .find(|m| m.original_name == "latest_per_user")
        .unwrap();
    assert_eq!(latest.layer, Layer::Marts);

    // ref() substitution works
    let ranked_sql = dbt_sql::generate_model_sql(ranked, &name_to_model);
    assert!(
        ranked_sql.contains("{{ ref('stg_base_events') }}"),
        "ranked_events should ref stg_base_events, got: {}",
        ranked_sql
    );

    // Window function syntax should be preserved in output
    let ranked_body = &ranked.sql_body;
    assert!(
        ranked_body.contains("ROW_NUMBER()"),
        "ROW_NUMBER should be preserved"
    );
    assert!(
        ranked_body.contains("PARTITION BY"),
        "PARTITION BY should be preserved"
    );
}

#[test]
fn test_union_ctes_end_to_end() {
    let (models, name_to_model) = run_pipeline("union_ctes.sql");

    // 4 CTEs + 1 final = 5 models
    assert_eq!(models.len(), 5);

    // us_orders and eu_orders are staging (no CTE deps)
    let us = models
        .iter()
        .find(|m| m.original_name == "us_orders")
        .unwrap();
    assert_eq!(us.layer, Layer::Staging);
    let eu = models
        .iter()
        .find(|m| m.original_name == "eu_orders")
        .unwrap();
    assert_eq!(eu.layer, Layer::Staging);

    // all_orders is intermediate (depends on both staging CTEs)
    let all = models
        .iter()
        .find(|m| m.original_name == "all_orders")
        .unwrap();
    assert_eq!(all.layer, Layer::Intermediate);
    assert!(all.dependencies.contains(&"us_orders".to_string()));
    assert!(all.dependencies.contains(&"eu_orders".to_string()));

    // ref() substitution in UNION ALL CTE
    let all_sql = dbt_sql::generate_model_sql(all, &name_to_model);
    assert!(
        all_sql.contains("{{ ref('stg_us_orders') }}"),
        "all_orders should ref stg_us_orders, got: {}",
        all_sql
    );
    assert!(
        all_sql.contains("{{ ref('stg_eu_orders') }}"),
        "all_orders should ref stg_eu_orders, got: {}",
        all_sql
    );

    // UNION ALL keyword should be preserved
    assert!(
        all.sql_body.contains("UNION ALL"),
        "UNION ALL should be preserved in body"
    );
}

#[test]
fn test_edge_cases_end_to_end() {
    let (models, name_to_model) = run_pipeline("edge_cases.sql");

    // 4 CTEs + 1 final = 5 models
    assert_eq!(models.len(), 5);

    // No-FROM CTE "constants" should have empty dependencies
    let constants = models
        .iter()
        .find(|m| m.original_name == "constants")
        .unwrap();
    assert!(
        constants.dependencies.is_empty(),
        "No-FROM CTE should have no deps, got: {:?}",
        constants.dependencies
    );

    // "filtered" should detect ref via WHERE IN subquery
    let filtered = models
        .iter()
        .find(|m| m.original_name == "filtered")
        .unwrap();
    assert!(
        filtered.dependencies.contains(&"independent".to_string()),
        "filtered should depend on independent"
    );
    assert!(
        filtered.dependencies.contains(&"constants".to_string()),
        "filtered should depend on constants via WHERE IN subquery"
    );

    // ref() substitution should work for the subquery-detected ref
    let filtered_sql = dbt_sql::generate_model_sql(filtered, &name_to_model);
    let constants_model_name = name_to_model.get("constants").unwrap();
    assert!(
        filtered_sql.contains(&format!("ref('{}')", constants_model_name)),
        "filtered should have ref() for constants, got: {}",
        filtered_sql
    );
}

#[test]
fn test_deep_chain_end_to_end() {
    let (models, name_to_model) = run_pipeline("deep_chain.sql");

    // 12 CTEs + 1 final = 13 models
    assert_eq!(models.len(), 13);

    // step_01 = staging (source node)
    let step_01 = models
        .iter()
        .find(|m| m.original_name == "step_01")
        .unwrap();
    assert_eq!(step_01.layer, Layer::Staging);

    // step_02 through step_11 = intermediate
    for i in 2..=11 {
        let name = format!("step_{:02}", i);
        let model = models.iter().find(|m| m.original_name == name).unwrap();
        assert_eq!(
            model.layer,
            Layer::Intermediate,
            "{} should be intermediate",
            name
        );
    }

    // step_12 = marts (terminal CTE node)
    let step_12 = models
        .iter()
        .find(|m| m.original_name == "step_12")
        .unwrap();
    assert_eq!(step_12.layer, Layer::Marts);

    // ref chain works: step_12 should ref step_11
    let step_12_sql = dbt_sql::generate_model_sql(step_12, &name_to_model);
    let step_11_model = name_to_model.get("step_11").unwrap();
    assert!(
        step_12_sql.contains(&format!("ref('{}')", step_11_model)),
        "step_12 should ref step_11's model name, got: {}",
        step_12_sql
    );
}

#[test]
fn test_cli_dry_run() {
    let bin = env!("CARGO_BIN_EXE_spaghetti-fix");
    let fixture = fixture_path("simple_ctes.sql");

    let output = std::process::Command::new(bin)
        .arg(fixture)
        .arg("--dry-run")
        .output()
        .expect("Failed to run binary");

    assert!(output.status.success(), "dry-run should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("stg_source.sql"),
        "Should list staging model file"
    );
    assert!(
        stdout.contains("int_cleaned.sql"),
        "Should list intermediate model file"
    );
    assert!(
        stdout.contains("final_output.sql"),
        "Should list marts model file"
    );
}

#[test]
fn test_cli_no_schema_flag() {
    let bin = env!("CARGO_BIN_EXE_spaghetti-fix");
    let fixture = fixture_path("simple_ctes.sql");
    let tmp = TempDir::new().unwrap();
    let output_dir = tmp.path().join("output");

    let output = std::process::Command::new(bin)
        .arg(&fixture)
        .arg("--output")
        .arg(&output_dir)
        .arg("--no-schema")
        .output()
        .expect("Failed to run binary");

    assert!(output.status.success(), "no-schema should exit 0");

    // .sql files should exist
    assert!(output_dir.join("models/staging/stg_source.sql").exists());
    // lineage.dot should exist
    assert!(output_dir.join("lineage.dot").exists());
    // schema.yml should NOT exist
    assert!(
        !output_dir
            .join("models/staging/_staging__schema.yml")
            .exists(),
        "Schema file should not exist with --no-schema"
    );
}

#[test]
fn test_cli_no_lineage_flag() {
    let bin = env!("CARGO_BIN_EXE_spaghetti-fix");
    let fixture = fixture_path("simple_ctes.sql");
    let tmp = TempDir::new().unwrap();
    let output_dir = tmp.path().join("output");

    let output = std::process::Command::new(bin)
        .arg(&fixture)
        .arg("--output")
        .arg(&output_dir)
        .arg("--no-lineage")
        .output()
        .expect("Failed to run binary");

    assert!(output.status.success(), "no-lineage should exit 0");

    // .sql files should exist
    assert!(output_dir.join("models/staging/stg_source.sql").exists());
    // schema.yml should exist
    assert!(output_dir
        .join("models/staging/_staging__schema.yml")
        .exists());
    // lineage.dot should NOT exist
    assert!(
        !output_dir.join("lineage.dot").exists(),
        "lineage.dot should not exist with --no-lineage"
    );
}
