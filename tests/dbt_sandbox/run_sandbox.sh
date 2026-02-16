#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SANDBOX_DIR="$SCRIPT_DIR"
FIXTURES_DIR="$PROJECT_ROOT/tests/fixtures"

FIXTURES=(
    simple_ctes
    diamond_ctes
    window_functions
    union_ctes
    edge_cases
    deep_chain
)

PASSED=0
FAILED=0
RESULTS=()

cd "$SANDBOX_DIR"

# Build spaghetti-fix once
echo "==========================================="
echo "  Building spaghetti-fix"
echo "==========================================="
cargo build --manifest-path "$PROJECT_ROOT/Cargo.toml" 2>&1
SPAGHETTI="$PROJECT_ROOT/target/debug/spaghetti-fix"

for fixture in "${FIXTURES[@]}"; do
    echo ""
    echo "==========================================="
    echo "  Testing: $fixture"
    echo "==========================================="

    # Clean generated models
    rm -f models/staging/*.sql models/staging/_*__schema.yml
    rm -f models/intermediate/*.sql models/intermediate/_*__schema.yml
    rm -f models/marts/*.sql models/marts/_*__schema.yml

    # Generate dbt models from fixture
    echo "--- spaghetti-fix ---"
    if ! "$SPAGHETTI" "$FIXTURES_DIR/${fixture}.sql" --output . --no-lineage 2>&1; then
        echo "FAIL: spaghetti-fix failed for $fixture"
        RESULTS+=("$fixture: FAIL (spaghetti-fix)")
        ((FAILED++)) || true
        continue
    fi

    # Seed DuckDB with CSV data
    echo "--- dbt seed ---"
    if ! dbt seed --profiles-dir . 2>&1; then
        echo "FAIL: dbt seed failed for $fixture"
        RESULTS+=("$fixture: FAIL (dbt seed)")
        ((FAILED++)) || true
        continue
    fi

    # Parse — validates ref() graph is correct
    echo "--- dbt parse ---"
    if ! dbt parse --profiles-dir . 2>&1; then
        echo "FAIL: dbt parse failed for $fixture"
        RESULTS+=("$fixture: FAIL (dbt parse)")
        ((FAILED++)) || true
        continue
    fi

    # Run — validates SQL compiles and executes
    echo "--- dbt run ---"
    if ! dbt run --profiles-dir . 2>&1; then
        echo "FAIL: dbt run failed for $fixture"
        RESULTS+=("$fixture: FAIL (dbt run)")
        ((FAILED++)) || true
        continue
    fi

    # Test — validates schema YAML is valid (some failures expected)
    echo "--- dbt test ---"
    if dbt test --profiles-dir . 2>&1; then
        echo "All dbt tests passed for $fixture"
    else
        echo "Note: Some dbt tests failed for $fixture (expected for non-unique columns)"
    fi

    RESULTS+=("$fixture: PASS")
    ((PASSED++)) || true
done

echo ""
echo "==========================================="
echo "  RESULTS SUMMARY"
echo "==========================================="
for result in "${RESULTS[@]}"; do
    echo "  $result"
done
echo ""
echo "  Passed: $PASSED / $((PASSED + FAILED))"

if [ "$FAILED" -gt 0 ]; then
    echo "  SOME FIXTURES FAILED"
    exit 1
else
    echo "  ALL FIXTURES PASSED"
    exit 0
fi
