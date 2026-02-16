use std::collections::HashSet;

use crate::graph::DependencyGraph;
use crate::model::Layer;

pub fn classify_layer(cte_name: &str, graph: &DependencyGraph) -> Layer {
    let sources: HashSet<String> = graph.source_nodes().into_iter().collect();
    let terminals: HashSet<String> = graph.terminal_nodes().into_iter().collect();

    if sources.contains(cte_name) {
        Layer::Staging
    } else if terminals.contains(cte_name) {
        Layer::Marts
    } else {
        Layer::Intermediate
    }
}

pub fn model_name(cte_name: &str, layer: &Layer) -> String {
    let prefix = layer.prefix();
    format!("{}{}", prefix, cte_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ExtractedCte;

    fn make_cte(name: &str, refs: &[&str]) -> ExtractedCte {
        ExtractedCte {
            name: name.to_string(),
            body: String::new(),
            table_refs: refs.iter().map(|s| s.to_string()).collect(),
            columns: Vec::new(),
            is_recursive: false,
        }
    }

    #[test]
    fn test_linear_layering() {
        let ctes = vec![
            make_cte("source", &[]),
            make_cte("cleaned", &["source"]),
            make_cte("final_output", &["cleaned"]),
        ];
        let graph = DependencyGraph::from_ctes(&ctes).unwrap();

        assert_eq!(classify_layer("source", &graph), Layer::Staging);
        assert_eq!(classify_layer("cleaned", &graph), Layer::Intermediate);
        assert_eq!(classify_layer("final_output", &graph), Layer::Marts);
    }

    #[test]
    fn test_model_name_prefixes() {
        assert_eq!(model_name("users", &Layer::Staging), "stg_users");
        assert_eq!(model_name("joined", &Layer::Intermediate), "int_joined");
        assert_eq!(model_name("revenue", &Layer::Marts), "revenue");
    }

    #[test]
    fn test_single_cte_is_staging_and_marts() {
        // A single CTE is both source and terminal — sources win (checked first)
        let ctes = vec![make_cte("only", &[])];
        let graph = DependencyGraph::from_ctes(&ctes).unwrap();
        assert_eq!(classify_layer("only", &graph), Layer::Staging);
    }

    #[test]
    fn test_diamond_layering() {
        // Diamond: a -> (b, c) -> d
        let ctes = vec![
            make_cte("a", &[]),
            make_cte("b", &["a"]),
            make_cte("c", &["a"]),
            make_cte("d", &["b", "c"]),
        ];
        let graph = DependencyGraph::from_ctes(&ctes).unwrap();

        assert_eq!(classify_layer("a", &graph), Layer::Staging);
        assert_eq!(classify_layer("b", &graph), Layer::Intermediate);
        assert_eq!(classify_layer("c", &graph), Layer::Intermediate);
        assert_eq!(classify_layer("d", &graph), Layer::Marts);
    }
}
