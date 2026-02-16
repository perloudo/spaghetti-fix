use std::collections::HashMap;

use anyhow::Result;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::error::SpaghettiError;
use crate::model::ExtractedCte;

#[derive(Debug)]
#[allow(dead_code)]
pub struct DependencyGraph {
    pub graph: DiGraph<String, ()>,
    pub node_map: HashMap<String, NodeIndex>,
}

impl DependencyGraph {
    pub fn from_ctes(ctes: &[ExtractedCte]) -> Result<Self> {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        // Add nodes
        for cte in ctes {
            let idx = graph.add_node(cte.name.clone());
            node_map.insert(cte.name.clone(), idx);
        }

        // Add edges: dependency -> dependent (so topo sort gives us deps first)
        for cte in ctes {
            let dependent = node_map[&cte.name];
            for dep in &cte.table_refs {
                if let Some(&dep_idx) = node_map.get(dep) {
                    graph.add_edge(dep_idx, dependent, ());
                }
            }
        }

        let dep_graph = DependencyGraph { graph, node_map };
        dep_graph.check_cycles()?;
        Ok(dep_graph)
    }

    fn check_cycles(&self) -> Result<()> {
        if toposort(&self.graph, None).is_err() {
            return Err(SpaghettiError::CycleDetected(
                "Dependency cycle detected in CTE graph".to_string(),
            )
            .into());
        }
        Ok(())
    }

    pub fn topological_order(&self) -> Result<Vec<String>> {
        let sorted = toposort(&self.graph, None).map_err(|cycle| {
            let node_name = &self.graph[cycle.node_id()];
            SpaghettiError::CycleDetected(format!("Cycle involving CTE: {}", node_name))
        })?;
        Ok(sorted
            .into_iter()
            .map(|idx| self.graph[idx].clone())
            .collect())
    }

    /// Returns names of CTEs that have no incoming edges (no CTE dependencies)
    pub fn source_nodes(&self) -> Vec<String> {
        self.graph
            .node_indices()
            .filter(|&idx| {
                self.graph
                    .neighbors_directed(idx, petgraph::Direction::Incoming)
                    .count()
                    == 0
            })
            .map(|idx| self.graph[idx].clone())
            .collect()
    }

    /// Returns names of CTEs that have no outgoing edges (no CTE dependents)
    pub fn terminal_nodes(&self) -> Vec<String> {
        self.graph
            .node_indices()
            .filter(|&idx| {
                self.graph
                    .neighbors_directed(idx, petgraph::Direction::Outgoing)
                    .count()
                    == 0
            })
            .map(|idx| self.graph[idx].clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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
    fn test_linear_topo_sort() {
        let ctes = vec![
            make_cte("a", &[]),
            make_cte("b", &["a"]),
            make_cte("c", &["b"]),
        ];
        let graph = DependencyGraph::from_ctes(&ctes).unwrap();
        let order = graph.topological_order().unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_diamond_topo_sort() {
        let ctes = vec![
            make_cte("a", &[]),
            make_cte("b", &["a"]),
            make_cte("c", &["a"]),
            make_cte("d", &["b", "c"]),
        ];
        let graph = DependencyGraph::from_ctes(&ctes).unwrap();
        let order = graph.topological_order().unwrap();

        // a must come first, d must come last
        assert_eq!(order[0], "a");
        assert_eq!(order[3], "d");
    }

    #[test]
    fn test_source_and_terminal_nodes() {
        let ctes = vec![
            make_cte("a", &[]),
            make_cte("b", &["a"]),
            make_cte("c", &["a"]),
            make_cte("d", &["b", "c"]),
        ];
        let graph = DependencyGraph::from_ctes(&ctes).unwrap();

        let sources: HashSet<String> = graph.source_nodes().into_iter().collect();
        assert_eq!(sources, HashSet::from(["a".to_string()]));

        let terminals: HashSet<String> = graph.terminal_nodes().into_iter().collect();
        assert_eq!(terminals, HashSet::from(["d".to_string()]));
    }

    #[test]
    fn test_deep_chain_topo_sort() {
        // 12-node linear chain
        let ctes: Vec<ExtractedCte> = (1..=12)
            .map(|i| {
                let name = format!("step_{:02}", i);
                let refs: Vec<&str> = if i == 1 {
                    vec![]
                } else {
                    // Can't return a reference to a local, so we use make_cte below
                    vec![]
                };
                let _ = refs;
                if i == 1 {
                    make_cte(&name, &[])
                } else {
                    let prev = format!("step_{:02}", i - 1);
                    ExtractedCte {
                        name,
                        body: String::new(),
                        table_refs: [prev].into_iter().collect(),
                        columns: Vec::new(),
                        is_recursive: false,
                    }
                }
            })
            .collect();

        let graph = DependencyGraph::from_ctes(&ctes).unwrap();
        let order = graph.topological_order().unwrap();

        assert_eq!(order.len(), 12);
        assert_eq!(order[0], "step_01");
        assert_eq!(order[11], "step_12");

        // Verify full ordering
        for i in 0..12 {
            assert_eq!(order[i], format!("step_{:02}", i + 1));
        }
    }

    #[test]
    fn test_disconnected_components() {
        // Two independent groups: (a->b) and (x->y)
        let ctes = vec![
            make_cte("a", &[]),
            make_cte("b", &["a"]),
            make_cte("x", &[]),
            make_cte("y", &["x"]),
        ];
        let graph = DependencyGraph::from_ctes(&ctes).unwrap();
        let order = graph.topological_order().unwrap();

        assert_eq!(order.len(), 4);

        // a must come before b
        let a_pos = order.iter().position(|n| n == "a").unwrap();
        let b_pos = order.iter().position(|n| n == "b").unwrap();
        assert!(a_pos < b_pos);

        // x must come before y
        let x_pos = order.iter().position(|n| n == "x").unwrap();
        let y_pos = order.iter().position(|n| n == "y").unwrap();
        assert!(x_pos < y_pos);

        // Sources should include both a and x
        let sources: HashSet<String> = graph.source_nodes().into_iter().collect();
        assert!(sources.contains("a"));
        assert!(sources.contains("x"));

        // Terminals should include both b and y
        let terminals: HashSet<String> = graph.terminal_nodes().into_iter().collect();
        assert!(terminals.contains("b"));
        assert!(terminals.contains("y"));
    }
}
