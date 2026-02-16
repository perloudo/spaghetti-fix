use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use clap::Parser;
use colored::*;
use log::info;

mod cli;
mod codegen;
mod error;
mod graph;
mod layering;
mod model;
mod parser;

use cli::Cli;
use codegen::dbt_sql;
use codegen::dot;
use codegen::schema_yml;
use graph::DependencyGraph;
use model::{DbtProject, Layer, Model};

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    }

    let project_name = cli.project_name.unwrap_or_else(|| {
        cli.input
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    });

    println!(
        "{} {}",
        "Spaghetti-Fix".bold().green(),
        format!("— parsing {}", cli.input.display()).dimmed()
    );

    // 1. Parse SQL
    let dialect = parser::dialect_from_str(&cli.dialect);
    let statements = parser::parse_sql_file(&cli.input, dialect.as_ref())?;
    info!("Parsed {} statement(s)", statements.len());

    // 2. Extract CTEs
    let (ctes, final_select) = parser::extract_ctes(&statements)?;

    // Filter out recursive CTEs with a warning
    let (recursive, regular): (Vec<_>, Vec<_>) = ctes.into_iter().partition(|c| c.is_recursive);
    for r in &recursive {
        eprintln!(
            "{} Recursive CTE '{}' detected — skipping",
            "Warning:".yellow().bold(),
            r.name
        );
    }

    if regular.is_empty() {
        return Err(error::SpaghettiError::NoCtes.into());
    }

    println!("  {} {} CTEs extracted", "✓".green(), regular.len());

    // 3. Build dependency graph
    let dep_graph = DependencyGraph::from_ctes(&regular)?;
    let topo_order = dep_graph.topological_order()?;
    info!("Topological order: {:?}", topo_order);

    // 4. Classify layers and build models
    let mut models = Vec::new();
    let mut name_to_model: HashMap<String, String> = HashMap::new();

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

    // Add final SELECT as a marts model if it exists
    if let Some(fs) = &final_select {
        let final_name = format!("{}_final", project_name);
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

    println!(
        "  {} Dependency graph built ({} nodes)",
        "✓".green(),
        models.len()
    );

    // Print layer summary
    let staging_count = models.iter().filter(|m| m.layer == Layer::Staging).count();
    let int_count = models
        .iter()
        .filter(|m| m.layer == Layer::Intermediate)
        .count();
    let marts_count = models.iter().filter(|m| m.layer == Layer::Marts).count();
    println!(
        "  {} Layers: {} staging, {} intermediate, {} marts",
        "✓".green(),
        staging_count,
        int_count,
        marts_count
    );

    // 5. Generate dbt SQL with ref() substitution
    let generated_sqls: Vec<String> = models
        .iter()
        .map(|m| dbt_sql::generate_model_sql(m, &name_to_model))
        .collect();

    if cli.dry_run {
        println!("\n{}", "Dry run — files that would be created:".yellow());
        print_file_tree(&cli.output, &models, !cli.no_schema, !cli.no_lineage);
        return Ok(());
    }

    // 6. Write output files
    let project = DbtProject {
        name: project_name,
        models: models.clone(),
        final_select: final_select.clone(),
    };

    write_project(
        &cli.output,
        &project,
        &generated_sqls,
        !cli.no_schema,
        !cli.no_lineage,
    )?;

    println!(
        "\n{} Output written to {}",
        "Done!".bold().green(),
        cli.output.display()
    );

    Ok(())
}

fn write_project(
    output_dir: &Path,
    project: &DbtProject,
    generated_sqls: &[String],
    write_schema: bool,
    write_lineage: bool,
) -> Result<()> {
    // Create layer directories
    let models_dir = output_dir.join("models");
    for layer in &[Layer::Staging, Layer::Intermediate, Layer::Marts] {
        let layer_dir = models_dir.join(layer.dir_name());
        fs::create_dir_all(&layer_dir)?;
    }

    // Write .sql files
    for (model, sql) in project.models.iter().zip(generated_sqls.iter()) {
        let layer_dir = models_dir.join(model.layer.dir_name());
        let file_path = layer_dir.join(format!("{}.sql", model.model_name));
        fs::write(&file_path, sql)?;
        info!("Wrote {}", file_path.display());
    }

    // Write schema.yml per layer
    if write_schema {
        for layer in &[Layer::Staging, Layer::Intermediate, Layer::Marts] {
            let layer_models: Vec<&Model> = project
                .models
                .iter()
                .filter(|m| &m.layer == layer)
                .collect();

            if layer_models.is_empty() {
                continue;
            }

            let schema_content = schema_yml::generate_schema_yml(&layer_models);
            let schema_path = models_dir
                .join(layer.dir_name())
                .join(schema_yml::schema_filename(layer));
            fs::write(&schema_path, schema_content)?;
            info!("Wrote {}", schema_path.display());
        }
    }

    // Write lineage.dot
    if write_lineage {
        let dot_content = dot::generate_dot(&project.models);
        let dot_path = output_dir.join("lineage.dot");
        fs::write(&dot_path, dot_content)?;
        info!("Wrote {}", dot_path.display());
    }

    Ok(())
}

fn print_file_tree(output_dir: &Path, models: &[Model], show_schema: bool, show_lineage: bool) {
    println!("  {}/", output_dir.display());
    println!("  └── models/");

    for layer in &[Layer::Staging, Layer::Intermediate, Layer::Marts] {
        let layer_models: Vec<&Model> = models.iter().filter(|m| &m.layer == layer).collect();
        if layer_models.is_empty() {
            continue;
        }

        println!("      ├── {}/", layer.dir_name());
        for (i, model) in layer_models.iter().enumerate() {
            let is_last = i == layer_models.len() - 1 && !show_schema;
            let prefix = if is_last { "└──" } else { "├──" };
            println!("      │   {} {}.sql", prefix, model.model_name);
        }
        if show_schema {
            println!("      │   └── {}", schema_yml::schema_filename(layer));
        }
    }

    if show_lineage {
        println!("  └── lineage.dot");
    }
}
