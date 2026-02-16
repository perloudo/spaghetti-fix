use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use log::debug;
use sqlparser::ast::{
    Expr, Query, Select, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins,
};
use sqlparser::dialect::{AnsiDialect, BigQueryDialect, Dialect, SnowflakeDialect};
use sqlparser::parser::Parser;

use crate::error::SpaghettiError;
use crate::model::{Column, ExtractedCte, FinalSelect};

pub fn dialect_from_str(s: &str) -> Box<dyn Dialect> {
    match s.to_lowercase().as_str() {
        "bigquery" => Box::new(BigQueryDialect),
        "snowflake" => Box::new(SnowflakeDialect),
        _ => Box::new(AnsiDialect {}),
    }
}

pub fn parse_sql_file(path: &Path, dialect: &dyn Dialect) -> Result<Vec<Statement>> {
    let sql = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read SQL file: {}", path.display()))?;
    parse_sql(&sql, dialect)
}

pub fn parse_sql(sql: &str, dialect: &dyn Dialect) -> Result<Vec<Statement>> {
    Parser::parse_sql(dialect, sql).map_err(|e| SpaghettiError::SqlParse(e.to_string()).into())
}

pub fn extract_ctes(statements: &[Statement]) -> Result<(Vec<ExtractedCte>, Option<FinalSelect>)> {
    let mut all_ctes = Vec::new();
    let mut final_select = None;

    for stmt in statements {
        if let Statement::Query(query) = stmt {
            if let Some(with) = &query.with {
                let cte_names: HashSet<String> = with
                    .cte_tables
                    .iter()
                    .map(|cte| cte.alias.name.value.to_lowercase())
                    .collect();

                for cte in &with.cte_tables {
                    let name = cte.alias.name.value.to_lowercase();
                    let body = cte.query.to_string();
                    let is_recursive = with.recursive;

                    let table_refs = extract_table_refs_from_query(&cte.query, &cte_names);
                    let columns = extract_columns_from_query(&cte.query);

                    debug!("CTE '{}': refs={:?}, cols={:?}", name, table_refs, columns);

                    all_ctes.push(ExtractedCte {
                        name,
                        body,
                        table_refs,
                        columns,
                        is_recursive,
                    });
                }

                // Extract the final SELECT (after the WITH clause)
                let final_refs = extract_table_refs_from_body(&query.body, &cte_names);
                let final_cols = extract_columns_from_body(&query.body);
                final_select = Some(FinalSelect {
                    body: query.body.to_string(),
                    table_refs: final_refs,
                    columns: final_cols,
                });
            }
        }
    }

    if all_ctes.is_empty() {
        return Err(SpaghettiError::NoCtes.into());
    }

    Ok((all_ctes, final_select))
}

fn extract_table_refs_from_query(query: &Query, cte_names: &HashSet<String>) -> HashSet<String> {
    extract_table_refs_from_body(&query.body, cte_names)
}

fn extract_table_refs_from_body(body: &SetExpr, cte_names: &HashSet<String>) -> HashSet<String> {
    let mut refs = HashSet::new();
    collect_table_refs_from_set_expr(body, cte_names, &mut refs);
    refs
}

fn collect_table_refs_from_set_expr(
    body: &SetExpr,
    cte_names: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    match body {
        SetExpr::Select(select) => {
            collect_table_refs_from_select(select, cte_names, refs);
        }
        SetExpr::Query(query) => {
            collect_table_refs_from_set_expr(&query.body, cte_names, refs);
        }
        SetExpr::SetOperation { left, right, .. } => {
            collect_table_refs_from_set_expr(left, cte_names, refs);
            collect_table_refs_from_set_expr(right, cte_names, refs);
        }
        SetExpr::Values(_) => {}
        _ => {}
    }
}

fn collect_table_refs_from_select(
    select: &Select,
    cte_names: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    for from in &select.from {
        collect_table_refs_from_table_with_joins(from, cte_names, refs);
    }
    // Also look in WHERE clause subqueries
    if let Some(selection) = &select.selection {
        collect_table_refs_from_expr(selection, cte_names, refs);
    }
}

fn collect_table_refs_from_table_with_joins(
    twj: &TableWithJoins,
    cte_names: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    collect_table_refs_from_table_factor(&twj.relation, cte_names, refs);
    for join in &twj.joins {
        collect_table_refs_from_table_factor(&join.relation, cte_names, refs);
    }
}

fn collect_table_refs_from_table_factor(
    factor: &TableFactor,
    cte_names: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    match factor {
        TableFactor::Table { name, .. } => {
            // Use only the last part of the name (handles schema.table)
            let table_name = name
                .0
                .last()
                .and_then(|p| p.as_ident())
                .map(|i| i.value.to_lowercase())
                .unwrap_or_default();
            if cte_names.contains(&table_name) {
                refs.insert(table_name);
            }
        }
        TableFactor::Derived { subquery, .. } => {
            collect_table_refs_from_set_expr(&subquery.body, cte_names, refs);
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            collect_table_refs_from_table_with_joins(table_with_joins, cte_names, refs);
        }
        _ => {}
    }
}

fn collect_table_refs_from_expr(
    expr: &Expr,
    cte_names: &HashSet<String>,
    refs: &mut HashSet<String>,
) {
    match expr {
        Expr::Subquery(query) => {
            collect_table_refs_from_set_expr(&query.body, cte_names, refs);
        }
        Expr::InSubquery { subquery, expr, .. } => {
            collect_table_refs_from_set_expr(&subquery.body, cte_names, refs);
            collect_table_refs_from_expr(expr, cte_names, refs);
        }
        Expr::Exists { subquery, .. } => {
            collect_table_refs_from_set_expr(&subquery.body, cte_names, refs);
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_table_refs_from_expr(left, cte_names, refs);
            collect_table_refs_from_expr(right, cte_names, refs);
        }
        Expr::UnaryOp { expr, .. } => {
            collect_table_refs_from_expr(expr, cte_names, refs);
        }
        Expr::Nested(inner) => {
            collect_table_refs_from_expr(inner, cte_names, refs);
        }
        _ => {}
    }
}

fn extract_columns_from_query(query: &Query) -> Vec<Column> {
    extract_columns_from_body(&query.body)
}

fn extract_columns_from_body(body: &SetExpr) -> Vec<Column> {
    match body {
        SetExpr::Select(select) => extract_columns_from_select(select),
        SetExpr::Query(query) => extract_columns_from_body(&query.body),
        _ => Vec::new(),
    }
}

fn extract_columns_from_select(select: &Select) -> Vec<Column> {
    let mut columns = Vec::new();
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(expr) => {
                if let Some(name) = expr_to_column_name(expr) {
                    columns.push(Column { name });
                }
            }
            SelectItem::ExprWithAlias { alias, .. } => {
                columns.push(Column {
                    name: alias.value.to_lowercase(),
                });
            }
            SelectItem::QualifiedWildcard(_, _) | SelectItem::Wildcard(_) => {
                // Can't infer columns from *
            }
        }
    }
    columns
}

fn expr_to_column_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(ident) => Some(ident.value.to_lowercase()),
        Expr::CompoundIdentifier(parts) => parts.last().map(|i| i.value.to_lowercase()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_extract(sql: &str) -> (Vec<ExtractedCte>, Option<FinalSelect>) {
        let dialect = AnsiDialect {};
        let stmts = parse_sql(sql, &dialect).unwrap();
        extract_ctes(&stmts).unwrap()
    }

    #[test]
    fn test_simple_linear_ctes() {
        let sql = r#"
            WITH source AS (
                SELECT id, name, created_at FROM raw_users
            ),
            cleaned AS (
                SELECT id AS user_id, UPPER(name) AS user_name, created_at
                FROM source
            ),
            final_output AS (
                SELECT user_id, user_name, created_at
                FROM cleaned
                WHERE user_id IS NOT NULL
            )
            SELECT * FROM final_output
        "#;
        let (ctes, final_sel) = parse_and_extract(sql);
        assert_eq!(ctes.len(), 3);

        assert_eq!(ctes[0].name, "source");
        assert!(ctes[0].table_refs.is_empty()); // raw_users is not a CTE

        assert_eq!(ctes[1].name, "cleaned");
        assert!(ctes[1].table_refs.contains("source"));

        assert_eq!(ctes[2].name, "final_output");
        assert!(ctes[2].table_refs.contains("cleaned"));

        // Columns from cleaned
        let col_names: Vec<&str> = ctes[1].columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"user_id"));
        assert!(col_names.contains(&"user_name"));
        assert!(col_names.contains(&"created_at"));

        // Final select
        assert!(final_sel.is_some());
        let fs = final_sel.unwrap();
        assert!(fs.table_refs.contains("final_output"));
    }

    #[test]
    fn test_diamond_deps() {
        let sql = r#"
            WITH a AS (
                SELECT id, val FROM raw_table
            ),
            b AS (
                SELECT id, val * 2 AS doubled FROM a
            ),
            c AS (
                SELECT id, val + 1 AS incremented FROM a
            ),
            d AS (
                SELECT b.id, b.doubled, c.incremented
                FROM b JOIN c ON b.id = c.id
            )
            SELECT * FROM d
        "#;
        let (ctes, _) = parse_and_extract(sql);
        assert_eq!(ctes.len(), 4);

        assert!(ctes[0].table_refs.is_empty()); // a
        assert!(ctes[1].table_refs.contains("a")); // b -> a
        assert!(ctes[2].table_refs.contains("a")); // c -> a
        assert!(ctes[3].table_refs.contains("b")); // d -> b, c
        assert!(ctes[3].table_refs.contains("c"));
    }

    #[test]
    fn test_no_ctes_error() {
        let sql = "SELECT 1";
        let dialect = AnsiDialect {};
        let stmts = parse_sql(sql, &dialect).unwrap();
        let result = extract_ctes(&stmts);
        assert!(result.is_err());
    }

    #[test]
    fn test_window_functions_parsed() {
        let sql = r#"
            WITH base AS (
                SELECT id, user_id, revenue, created_at FROM raw_events
            ),
            ranked AS (
                SELECT
                    id,
                    user_id,
                    revenue,
                    ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY created_at DESC) AS row_num,
                    RANK() OVER (PARTITION BY user_id ORDER BY revenue DESC) AS revenue_rank,
                    LAG(created_at) OVER (PARTITION BY user_id ORDER BY created_at) AS prev_at,
                    LEAD(created_at) OVER (PARTITION BY user_id ORDER BY created_at) AS next_at
                FROM base
            )
            SELECT * FROM ranked
        "#;
        let (ctes, _) = parse_and_extract(sql);
        assert_eq!(ctes.len(), 2);

        // Window functions should not create spurious table refs
        assert_eq!(ctes[1].name, "ranked");
        assert_eq!(ctes[1].table_refs.len(), 1);
        assert!(ctes[1].table_refs.contains("base"));

        // Aliased columns from window functions should be extracted
        let col_names: Vec<&str> = ctes[1].columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"row_num"));
        assert!(col_names.contains(&"revenue_rank"));
        assert!(col_names.contains(&"prev_at"));
        assert!(col_names.contains(&"next_at"));
    }

    #[test]
    fn test_union_all_in_cte() {
        let sql = r#"
            WITH a AS (
                SELECT id FROM raw_a
            ),
            b AS (
                SELECT id FROM raw_b
            ),
            combined AS (
                SELECT id FROM a
                UNION ALL
                SELECT id FROM b
            )
            SELECT * FROM combined
        "#;
        let (ctes, _) = parse_and_extract(sql);
        assert_eq!(ctes.len(), 3);

        // UNION ALL body should find refs in both branches
        assert_eq!(ctes[2].name, "combined");
        assert!(ctes[2].table_refs.contains("a"));
        assert!(ctes[2].table_refs.contains("b"));
        assert_eq!(ctes[2].table_refs.len(), 2);
    }

    #[test]
    fn test_cte_with_no_from() {
        let sql = r#"
            WITH constants AS (
                SELECT 1 AS id, 'hello' AS label
            ),
            uses_constants AS (
                SELECT id, label FROM constants
            )
            SELECT * FROM uses_constants
        "#;
        let (ctes, _) = parse_and_extract(sql);
        assert_eq!(ctes.len(), 2);

        // No-FROM CTE should have 0 table refs
        assert_eq!(ctes[0].name, "constants");
        assert!(ctes[0].table_refs.is_empty());

        // Should still extract columns
        let col_names: Vec<&str> = ctes[0].columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"id"));
        assert!(col_names.contains(&"label"));
    }

    #[test]
    fn test_subquery_in_where_refs() {
        let sql = r#"
            WITH src AS (
                SELECT id, val FROM raw_data
            ),
            filtered AS (
                SELECT id, val
                FROM src
                WHERE id IN (SELECT id FROM src WHERE val > 10)
            )
            SELECT * FROM filtered
        "#;
        let (ctes, _) = parse_and_extract(sql);
        assert_eq!(ctes.len(), 2);

        // WHERE IN subquery should detect the CTE ref
        assert_eq!(ctes[1].name, "filtered");
        assert!(ctes[1].table_refs.contains("src"));
    }
}
