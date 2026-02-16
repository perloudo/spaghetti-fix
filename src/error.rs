use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum SpaghettiError {
    #[error("Failed to read SQL file: {0}")]
    FileRead(#[from] std::io::Error),

    #[error("SQL parsing error: {0}")]
    SqlParse(String),

    #[error("No CTEs found in the input SQL file")]
    NoCtes,

    #[error("Dependency cycle detected: {0}")]
    CycleDetected(String),

    #[error("Recursive CTE detected: {0} — skipping")]
    RecursiveCte(String),

    #[error("Code generation error: {0}")]
    CodeGen(String),
}
