-- Real-world e-commerce analytics query (500+ lines)
-- Combines user data, orders, products, payments, and shipping into final revenue analysis

WITH raw_users AS (
    SELECT
        u.user_id,
        u.first_name,
        u.last_name,
        u.email,
        u.phone,
        u.country,
        u.state,
        u.city,
        u.zip_code,
        u.registration_date,
        u.last_login_date,
        u.is_active,
        u.user_type,
        u.referral_source,
        u.lifetime_value_tier
    FROM analytics.public.users u
    WHERE u.is_deleted = FALSE
),

raw_orders AS (
    SELECT
        o.order_id,
        o.user_id,
        o.order_date,
        o.order_status,
        o.shipping_method,
        o.shipping_cost,
        o.discount_code,
        o.discount_amount,
        o.subtotal,
        o.tax_amount,
        o.total_amount,
        o.currency,
        o.payment_method,
        o.billing_address_id,
        o.shipping_address_id,
        o.created_at,
        o.updated_at
    FROM analytics.public.orders o
    WHERE o.is_test_order = FALSE
      AND o.order_date >= '2023-01-01'
),

raw_order_items AS (
    SELECT
        oi.order_item_id,
        oi.order_id,
        oi.product_id,
        oi.variant_id,
        oi.quantity,
        oi.unit_price,
        oi.line_total,
        oi.discount_applied,
        oi.return_status,
        oi.fulfilled_at
    FROM analytics.public.order_items oi
),

raw_products AS (
    SELECT
        p.product_id,
        p.product_name,
        p.category_id,
        p.subcategory,
        p.brand,
        p.sku,
        p.cost_price,
        p.retail_price,
        p.weight_kg,
        p.is_active AS product_is_active,
        p.created_at AS product_created_at
    FROM analytics.public.products p
),

raw_categories AS (
    SELECT
        c.category_id,
        c.category_name,
        c.parent_category_id,
        c.category_level,
        c.is_leaf
    FROM analytics.public.categories c
),

raw_payments AS (
    SELECT
        pm.payment_id,
        pm.order_id,
        pm.payment_method,
        pm.payment_status,
        pm.payment_amount,
        pm.payment_date,
        pm.transaction_id,
        pm.gateway_response_code,
        pm.is_refund,
        pm.refund_reason
    FROM analytics.public.payments pm
),

raw_shipping AS (
    SELECT
        s.shipping_id,
        s.order_id,
        s.carrier,
        s.tracking_number,
        s.shipped_date,
        s.estimated_delivery_date,
        s.actual_delivery_date,
        s.delivery_status,
        s.shipping_cost AS actual_shipping_cost
    FROM analytics.public.shipping s
),

-- Staging: clean and standardize
stg_users AS (
    SELECT
        user_id,
        TRIM(first_name) AS first_name,
        TRIM(last_name) AS last_name,
        TRIM(first_name) || ' ' || TRIM(last_name) AS full_name,
        LOWER(TRIM(email)) AS email,
        phone,
        UPPER(country) AS country,
        state,
        city,
        zip_code,
        registration_date,
        last_login_date,
        is_active,
        user_type,
        referral_source,
        lifetime_value_tier,
        CASE
            WHEN last_login_date >= CURRENT_DATE - INTERVAL '30' DAY THEN 'active'
            WHEN last_login_date >= CURRENT_DATE - INTERVAL '90' DAY THEN 'at_risk'
            ELSE 'churned'
        END AS engagement_status
    FROM raw_users
),

stg_orders AS (
    SELECT
        order_id,
        user_id,
        order_date,
        order_status,
        shipping_method,
        shipping_cost,
        discount_code,
        discount_amount,
        subtotal,
        tax_amount,
        total_amount,
        currency,
        payment_method,
        billing_address_id,
        shipping_address_id,
        created_at,
        updated_at,
        CASE
            WHEN order_status = 'completed' THEN 1
            WHEN order_status = 'shipped' THEN 1
            WHEN order_status = 'processing' THEN 1
            ELSE 0
        END AS is_valid_order,
        CASE
            WHEN discount_amount > 0 THEN TRUE
            ELSE FALSE
        END AS has_discount
    FROM raw_orders
),

stg_order_items AS (
    SELECT
        order_item_id,
        order_id,
        product_id,
        variant_id,
        quantity,
        unit_price,
        line_total,
        discount_applied,
        return_status,
        fulfilled_at,
        CASE
            WHEN return_status IN ('returned', 'refunded') THEN TRUE
            ELSE FALSE
        END AS is_returned
    FROM raw_order_items
),

stg_products AS (
    SELECT
        p.product_id,
        p.product_name,
        p.category_id,
        p.subcategory,
        p.brand,
        p.sku,
        p.cost_price,
        p.retail_price,
        p.weight_kg,
        p.product_is_active,
        p.product_created_at,
        c.category_name,
        c.parent_category_id,
        c.category_level,
        p.retail_price - p.cost_price AS margin,
        CASE
            WHEN p.retail_price > 0
            THEN (p.retail_price - p.cost_price) / p.retail_price * 100
            ELSE 0
        END AS margin_pct
    FROM raw_products p
    LEFT JOIN raw_categories c ON p.category_id = c.category_id
),

stg_payments AS (
    SELECT
        payment_id,
        order_id,
        payment_method,
        payment_status,
        payment_amount,
        payment_date,
        transaction_id,
        gateway_response_code,
        is_refund,
        refund_reason,
        CASE
            WHEN payment_status = 'completed' THEN payment_amount
            ELSE 0
        END AS confirmed_amount,
        CASE
            WHEN is_refund = TRUE THEN payment_amount
            ELSE 0
        END AS refunded_amount
    FROM raw_payments
),

stg_shipping AS (
    SELECT
        shipping_id,
        order_id,
        carrier,
        tracking_number,
        shipped_date,
        estimated_delivery_date,
        actual_delivery_date,
        delivery_status,
        actual_shipping_cost,
        CASE
            WHEN actual_delivery_date IS NOT NULL AND estimated_delivery_date IS NOT NULL
            THEN actual_delivery_date - estimated_delivery_date
            ELSE NULL
        END AS delivery_delay_days,
        CASE
            WHEN delivery_status = 'delivered' THEN TRUE
            ELSE FALSE
        END AS is_delivered
    FROM raw_shipping
),

-- Intermediate: join and aggregate
int_order_details AS (
    SELECT
        o.order_id,
        o.user_id,
        o.order_date,
        o.order_status,
        o.shipping_method,
        o.shipping_cost,
        o.discount_code,
        o.discount_amount,
        o.subtotal,
        o.tax_amount,
        o.total_amount,
        o.currency,
        o.payment_method AS order_payment_method,
        o.is_valid_order,
        o.has_discount,
        COUNT(oi.order_item_id) AS item_count,
        SUM(oi.quantity) AS total_quantity,
        SUM(oi.line_total) AS items_total,
        SUM(CASE WHEN oi.is_returned THEN oi.line_total ELSE 0 END) AS returned_amount,
        SUM(CASE WHEN oi.is_returned THEN 1 ELSE 0 END) AS returned_items,
        COUNT(DISTINCT oi.product_id) AS unique_products
    FROM stg_orders o
    LEFT JOIN stg_order_items oi ON o.order_id = oi.order_id
    GROUP BY
        o.order_id,
        o.user_id,
        o.order_date,
        o.order_status,
        o.shipping_method,
        o.shipping_cost,
        o.discount_code,
        o.discount_amount,
        o.subtotal,
        o.tax_amount,
        o.total_amount,
        o.currency,
        o.payment_method,
        o.is_valid_order,
        o.has_discount
),

int_order_payments AS (
    SELECT
        od.order_id,
        od.user_id,
        od.order_date,
        od.order_status,
        od.total_amount,
        od.item_count,
        od.total_quantity,
        od.returned_amount,
        od.unique_products,
        od.is_valid_order,
        COALESCE(SUM(p.confirmed_amount), 0) AS total_paid,
        COALESCE(SUM(p.refunded_amount), 0) AS total_refunded,
        COUNT(DISTINCT p.payment_id) AS payment_count,
        MAX(p.payment_date) AS last_payment_date,
        od.total_amount - COALESCE(SUM(p.confirmed_amount), 0) AS payment_gap
    FROM int_order_details od
    LEFT JOIN stg_payments p ON od.order_id = p.order_id
    GROUP BY
        od.order_id,
        od.user_id,
        od.order_date,
        od.order_status,
        od.total_amount,
        od.item_count,
        od.total_quantity,
        od.returned_amount,
        od.unique_products,
        od.is_valid_order
),

int_order_shipping AS (
    SELECT
        op.order_id,
        op.user_id,
        op.order_date,
        op.order_status,
        op.total_amount,
        op.total_paid,
        op.total_refunded,
        op.item_count,
        op.unique_products,
        op.is_valid_order,
        s.carrier,
        s.shipped_date,
        s.estimated_delivery_date,
        s.actual_delivery_date,
        s.delivery_status,
        s.delivery_delay_days,
        s.is_delivered,
        s.actual_shipping_cost
    FROM int_order_payments op
    LEFT JOIN stg_shipping s ON op.order_id = s.order_id
),

int_user_order_summary AS (
    SELECT
        u.user_id,
        u.full_name,
        u.email,
        u.country,
        u.registration_date,
        u.engagement_status,
        u.user_type,
        u.referral_source,
        COUNT(DISTINCT os.order_id) AS total_orders,
        COUNT(DISTINCT CASE WHEN os.is_valid_order = 1 THEN os.order_id END) AS valid_orders,
        COALESCE(SUM(os.total_amount), 0) AS gross_revenue,
        COALESCE(SUM(os.total_paid), 0) AS net_paid,
        COALESCE(SUM(os.total_refunded), 0) AS total_refunds,
        COALESCE(SUM(os.total_paid) - SUM(os.total_refunded), 0) AS net_revenue,
        COALESCE(AVG(os.total_amount), 0) AS avg_order_value,
        MIN(os.order_date) AS first_order_date,
        MAX(os.order_date) AS last_order_date,
        COALESCE(SUM(os.item_count), 0) AS total_items_ordered,
        COALESCE(SUM(os.unique_products), 0) AS total_unique_products,
        COUNT(DISTINCT CASE WHEN os.is_delivered THEN os.order_id END) AS delivered_orders,
        COALESCE(AVG(os.delivery_delay_days), 0) AS avg_delivery_delay
    FROM stg_users u
    LEFT JOIN int_order_shipping os ON u.user_id = os.user_id
    GROUP BY
        u.user_id,
        u.full_name,
        u.email,
        u.country,
        u.registration_date,
        u.engagement_status,
        u.user_type,
        u.referral_source
),

int_product_performance AS (
    SELECT
        p.product_id,
        p.product_name,
        p.category_name,
        p.brand,
        p.cost_price,
        p.retail_price,
        p.margin,
        p.margin_pct,
        COUNT(DISTINCT oi.order_id) AS orders_with_product,
        SUM(oi.quantity) AS total_units_sold,
        SUM(oi.line_total) AS total_revenue,
        SUM(oi.quantity * p.cost_price) AS total_cost,
        SUM(oi.line_total) - SUM(oi.quantity * p.cost_price) AS total_profit,
        CASE
            WHEN SUM(oi.line_total) > 0
            THEN (SUM(oi.line_total) - SUM(oi.quantity * p.cost_price)) / SUM(oi.line_total) * 100
            ELSE 0
        END AS profit_margin_pct,
        SUM(CASE WHEN oi.is_returned THEN oi.quantity ELSE 0 END) AS returned_units,
        CASE
            WHEN SUM(oi.quantity) > 0
            THEN SUM(CASE WHEN oi.is_returned THEN oi.quantity ELSE 0 END) * 100.0 / SUM(oi.quantity)
            ELSE 0
        END AS return_rate_pct
    FROM stg_products p
    LEFT JOIN stg_order_items oi ON p.product_id = oi.product_id
    GROUP BY
        p.product_id,
        p.product_name,
        p.category_name,
        p.brand,
        p.cost_price,
        p.retail_price,
        p.margin,
        p.margin_pct
),

-- Marts: final business models
mart_revenue_summary AS (
    SELECT
        uos.user_id,
        uos.full_name,
        uos.email,
        uos.country,
        uos.registration_date,
        uos.engagement_status,
        uos.user_type,
        uos.referral_source,
        uos.total_orders,
        uos.valid_orders,
        uos.gross_revenue,
        uos.net_paid,
        uos.total_refunds,
        uos.net_revenue,
        uos.avg_order_value,
        uos.first_order_date,
        uos.last_order_date,
        uos.total_items_ordered,
        uos.delivered_orders,
        uos.avg_delivery_delay,
        CASE
            WHEN uos.net_revenue >= 10000 THEN 'platinum'
            WHEN uos.net_revenue >= 5000 THEN 'gold'
            WHEN uos.net_revenue >= 1000 THEN 'silver'
            ELSE 'bronze'
        END AS revenue_tier,
        CASE
            WHEN uos.total_orders >= 10 THEN 'loyal'
            WHEN uos.total_orders >= 5 THEN 'regular'
            WHEN uos.total_orders >= 2 THEN 'returning'
            ELSE 'one_time'
        END AS loyalty_segment,
        CASE
            WHEN uos.last_order_date >= CURRENT_DATE - INTERVAL '30' DAY THEN 'recent'
            WHEN uos.last_order_date >= CURRENT_DATE - INTERVAL '90' DAY THEN 'lapsing'
            ELSE 'dormant'
        END AS recency_segment
    FROM int_user_order_summary uos
    WHERE uos.total_orders > 0
),

mart_product_report AS (
    SELECT
        pp.product_id,
        pp.product_name,
        pp.category_name,
        pp.brand,
        pp.cost_price,
        pp.retail_price,
        pp.margin,
        pp.margin_pct,
        pp.orders_with_product,
        pp.total_units_sold,
        pp.total_revenue,
        pp.total_cost,
        pp.total_profit,
        pp.profit_margin_pct,
        pp.returned_units,
        pp.return_rate_pct,
        CASE
            WHEN pp.total_units_sold >= 1000 THEN 'high_volume'
            WHEN pp.total_units_sold >= 100 THEN 'medium_volume'
            ELSE 'low_volume'
        END AS volume_tier,
        CASE
            WHEN pp.profit_margin_pct >= 50 THEN 'high_margin'
            WHEN pp.profit_margin_pct >= 20 THEN 'medium_margin'
            ELSE 'low_margin'
        END AS margin_tier
    FROM int_product_performance pp
    WHERE pp.total_units_sold > 0
)

SELECT
    rs.user_id,
    rs.full_name,
    rs.country,
    rs.revenue_tier,
    rs.loyalty_segment,
    rs.recency_segment,
    rs.net_revenue,
    rs.total_orders,
    rs.avg_order_value,
    rs.delivered_orders,
    rs.avg_delivery_delay
FROM mart_revenue_summary rs
ORDER BY rs.net_revenue DESC
