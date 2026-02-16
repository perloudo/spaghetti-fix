use crate::model::{Layer, Model};

pub fn generate_dot(models: &[Model]) -> String {
    let mut lines = vec![
        "digraph lineage {".to_string(),
        "    rankdir=LR;".to_string(),
        "    node [shape=box, style=filled];".to_string(),
        String::new(),
    ];

    // Define nodes with layer-based coloring
    for model in models {
        let color = match model.layer {
            Layer::Staging => "#a8d5e2",      // light blue
            Layer::Intermediate => "#f9e79f", // light yellow
            Layer::Marts => "#a9dfbf",        // light green
        };
        lines.push(format!(
            "    \"{}\" [fillcolor=\"{}\", label=\"{}\\n({})\"];",
            model.model_name, color, model.model_name, model.layer
        ));
    }

    lines.push(String::new());

    // Build a map from original CTE name to model name for edge resolution
    let name_to_model: std::collections::HashMap<&str, &str> = models
        .iter()
        .map(|m| (m.original_name.as_str(), m.model_name.as_str()))
        .collect();

    // Define edges
    for model in models {
        for dep in &model.dependencies {
            if let Some(dep_model_name) = name_to_model.get(dep.as_str()) {
                lines.push(format!(
                    "    \"{}\" -> \"{}\";",
                    dep_model_name, model.model_name
                ));
            }
        }
    }

    lines.push("}".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;

    #[test]
    fn test_dot_output() {
        let models = vec![
            Model {
                original_name: "source".to_string(),
                model_name: "stg_source".to_string(),
                layer: Layer::Staging,
                sql_body: String::new(),
                columns: Vec::new(),
                dependencies: vec![],
            },
            Model {
                original_name: "final".to_string(),
                model_name: "final".to_string(),
                layer: Layer::Marts,
                sql_body: String::new(),
                columns: Vec::new(),
                dependencies: vec!["source".to_string()],
            },
        ];

        let dot = generate_dot(&models);
        assert!(dot.contains("digraph lineage"));
        assert!(dot.contains("stg_source"));
        assert!(dot.contains("\"stg_source\" -> \"final\""));
        assert!(dot.contains("#a8d5e2")); // staging color
        assert!(dot.contains("#a9dfbf")); // marts color
    }
}
