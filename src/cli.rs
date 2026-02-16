use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "spaghetti-fix",
    about = "Split monolithic SQL into modular dbt models",
    version
)]
pub struct Cli {
    /// Input SQL file path
    pub input: PathBuf,

    /// Output directory
    #[arg(short, long, default_value = "./dbt_output")]
    pub output: PathBuf,

    /// SQL dialect: ansi, bigquery, snowflake
    #[arg(short, long, default_value = "ansi")]
    pub dialect: String,

    /// Project name (defaults to input file stem)
    #[arg(short, long)]
    pub project_name: Option<String>,

    /// Skip schema.yml generation
    #[arg(long)]
    pub no_schema: bool,

    /// Skip DOT lineage generation
    #[arg(long)]
    pub no_lineage: bool,

    /// Print file tree without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Show parsing details
    #[arg(short, long)]
    pub verbose: bool,
}
