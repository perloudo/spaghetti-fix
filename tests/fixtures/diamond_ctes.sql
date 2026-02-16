-- Diamond DAG: base -> orders + customers -> combined
WITH base_data AS (
    SELECT
        order_id,
        customer_id,
        order_date,
        amount
    FROM raw_orders
),

orders AS (
    SELECT
        order_id,
        customer_id,
        order_date,
        amount,
        amount * 0.1 AS tax
    FROM base_data
    WHERE amount > 0
),

customers AS (
    SELECT
        customer_id,
        COUNT(order_id) AS order_count,
        SUM(amount) AS total_spent
    FROM base_data
    GROUP BY customer_id
),

combined AS (
    SELECT
        o.order_id,
        o.customer_id,
        o.order_date,
        o.amount,
        o.tax,
        c.order_count,
        c.total_spent
    FROM orders o
    JOIN customers c ON o.customer_id = c.customer_id
)

SELECT * FROM combined
