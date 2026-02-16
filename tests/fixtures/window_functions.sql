-- Window functions: base_events -> ranked_events (ROW_NUMBER, RANK, LAG, LEAD) -> latest_per_user
WITH base_events AS (
    SELECT
        event_id,
        user_id,
        event_type,
        revenue,
        created_at
    FROM raw_events
),

ranked_events AS (
    SELECT
        event_id,
        user_id,
        event_type,
        revenue,
        created_at,
        ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY created_at DESC) AS row_num,
        RANK() OVER (PARTITION BY user_id ORDER BY revenue DESC) AS revenue_rank,
        LAG(created_at) OVER (PARTITION BY user_id ORDER BY created_at) AS prev_event_at,
        LEAD(created_at) OVER (PARTITION BY user_id ORDER BY created_at) AS next_event_at
    FROM base_events
),

latest_per_user AS (
    SELECT
        event_id,
        user_id,
        event_type,
        revenue,
        created_at,
        revenue_rank,
        prev_event_at
    FROM ranked_events
    WHERE row_num = 1
)

SELECT * FROM latest_per_user
