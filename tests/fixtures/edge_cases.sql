-- Edge cases: no-FROM CTE, dependent on literal CTE, independent CTE, WHERE IN subquery
WITH constants AS (
    SELECT 1 AS id, 'default' AS label
),

derived_from_constants AS (
    SELECT
        id AS const_id,
        label AS const_label,
        'extra' AS extra_field
    FROM constants
),

independent AS (
    SELECT
        product_id,
        product_name,
        price
    FROM raw_products
),

filtered AS (
    SELECT
        product_id,
        product_name,
        price
    FROM independent
    WHERE product_id IN (SELECT id FROM constants)
)

SELECT * FROM filtered
