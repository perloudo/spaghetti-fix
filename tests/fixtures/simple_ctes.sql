-- Simple linear CTE chain: source -> cleaned -> final
WITH source AS (
    SELECT
        id,
        name,
        email,
        created_at
    FROM raw_users
),

cleaned AS (
    SELECT
        id AS user_id,
        UPPER(name) AS user_name,
        LOWER(email) AS user_email,
        created_at
    FROM source
    WHERE id IS NOT NULL
),

final_output AS (
    SELECT
        user_id,
        user_name,
        user_email,
        created_at
    FROM cleaned
    WHERE user_email LIKE '%@%'
)

SELECT * FROM final_output
