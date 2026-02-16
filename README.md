# spaghetti-fix

Split monolithic SQL into modular dbt projects.

Takes a single SQL file full of CTEs and outputs a structured dbt project with properly layered models, `ref()` calls, schema YAML files, and a DOT lineage graph.

## Demo

**Input** — one big SQL file with nested CTEs:

```sql
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
    WHERE user_email LIKE '%@%'
)
SELECT * FROM final_output
```

**Output** — a structured dbt project:

```
dbt_output/
  models/
    staging/
      stg_source.sql
      _staging__schema.yml
    intermediate/
      int_cleaned.sql
      _intermediate__schema.yml
    marts/
      final_output.sql
      _marts__schema.yml
  lineage.dot
```

Each model uses `ref()` instead of raw CTE names:

```sql
-- models/intermediate/int_cleaned.sql
SELECT id AS user_id, UPPER(name) AS user_name, created_at
FROM {{ ref('stg_source') }}
```

## Installation

```bash
cargo install --path .
```

## Usage

```
Split monolithic SQL into modular dbt models

Usage: spaghetti-fix [OPTIONS] <INPUT>

Arguments:
  <INPUT>  Input SQL file path

Options:
  -o, --output <OUTPUT>              Output directory [default: ./dbt_output]
  -d, --dialect <DIALECT>            SQL dialect: ansi, bigquery, snowflake [default: ansi]
  -p, --project-name <PROJECT_NAME>  Project name (defaults to input file stem)
      --no-schema                    Skip schema.yml generation
      --no-lineage                   Skip DOT lineage generation
      --dry-run                      Print file tree without writing
  -v, --verbose                      Show parsing details
  -h, --help                         Print help
  -V, --version                      Print version
```

### Examples

Basic usage:

```bash
spaghetti-fix query.sql
```

Preview output without writing files:

```bash
spaghetti-fix query.sql --dry-run
```

Snowflake dialect with custom output directory:

```bash
spaghetti-fix query.sql --dialect snowflake --output ./my_dbt_project
```

Generate models without lineage graph:

```bash
spaghetti-fix query.sql --no-lineage
```

Generate models without schema YAML:

```bash
spaghetti-fix query.sql --no-schema
```

## How It Works

```
Input SQL
    |
    v
[1. Parse SQL]          -- sqlparser-rs with dialect support
    |
    v
[2. Extract CTEs]       -- name, body, column aliases, table references
    |
    v
[3. Build DAG]          -- petgraph directed graph, topological sort
    |
    v
[4. Classify Layers]    -- staging / intermediate / marts based on graph position
    |
    v
[5. Generate Output]    -- dbt SQL with ref(), schema.yml, lineage.dot
```

1. **Parse** — Uses `sqlparser-rs` to parse the input SQL file into an AST. Supports ANSI, BigQuery, and Snowflake dialects.
2. **Extract CTEs** — Walks the AST to extract each CTE's name, SQL body, projected columns, and table references (including references inside JOINs, subqueries, and UNION ALL branches).
3. **Build DAG** — Constructs a directed acyclic graph where edges represent CTE-to-CTE dependencies. Detects cycles and reports errors.
4. **Classify Layers** — Assigns each CTE to a dbt layer based on its position in the DAG:
   - **Staging** — source nodes (no CTE dependencies)
   - **Intermediate** — middle nodes (have both upstream and downstream CTE connections)
   - **Marts** — terminal nodes (no CTE dependents)
5. **Generate Output** — Produces dbt-compatible SQL files with `ref()` substitution, per-layer `schema.yml` with inferred tests, and a DOT lineage graph.

## Output Structure

```
<output_dir>/
  models/
    staging/
      stg_<cte_name>.sql          -- Source CTEs (no CTE dependencies)
      _staging__schema.yml
    intermediate/
      int_<cte_name>.sql          -- Middle CTEs
      _intermediate__schema.yml
    marts/
      <cte_name>.sql              -- Terminal CTEs (no prefix)
      <project>_final.sql         -- Final SELECT after WITH block
      _marts__schema.yml
  lineage.dot                     -- Graphviz DAG visualization
```

### Naming conventions

| Layer | Prefix | Example |
|-------|--------|---------|
| Staging | `stg_` | `stg_raw_users` |
| Intermediate | `int_` | `int_cleaned_users` |
| Marts | *(none)* | `revenue_summary` |

## Test Inference Rules

Schema YAML files include automatically inferred tests based on column naming patterns:

| Column Pattern | Inferred Tests |
|---------------|----------------|
| `*_id` or `id` | `unique`, `not_null` |
| `*_at` | `not_null` |
| `*_date` | `not_null` |
| `*_timestamp` | `not_null` |

## Limitations

- **Scaffolder only** — Generates a starting point; manual refinement of models, tests, and documentation is expected.
- **Recursive CTEs skipped** — Recursive CTEs are detected and skipped with a warning.
- **No column-level lineage** — Tracks table-level dependencies only. Column lineage is not traced through transformations.
- **Heuristic layering** — Layer classification is based on graph topology (source/middle/terminal), not semantic analysis. Complex DAGs may need manual reclassification.
- **Single-file input** — Processes one SQL file at a time. Multi-file projects require running the tool per file.

## Contributing

```bash
# Run all tests
cargo test

# Run with verbose output
cargo test -- --nocapture

# Test fixtures are in tests/fixtures/
ls tests/fixtures/
```

## License

MIT
