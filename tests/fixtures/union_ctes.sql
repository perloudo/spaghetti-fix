-- UNION ALL: us_orders + eu_orders (independent) -> all_orders (UNION ALL) -> order_summary
WITH us_orders AS (
    SELECT
        order_id,
        customer_id,
        amount,
        order_date,
        'US' AS region
    FROM raw_us_orders
),

eu_orders AS (
    SELECT
        order_id,
        customer_id,
        amount,
        order_date,
        'EU' AS region
    FROM raw_eu_orders
),

all_orders AS (
    SELECT order_id, customer_id, amount, order_date, region
    FROM us_orders
    UNION ALL
    SELECT order_id, customer_id, amount, order_date, region
    FROM eu_orders
),

order_summary AS (
    SELECT
        region,
        COUNT(order_id) AS order_count,
        SUM(amount) AS total_revenue
    FROM all_orders
    GROUP BY region
)

SELECT * FROM order_summary
