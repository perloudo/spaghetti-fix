-- Deep linear chain: step_01 -> step_02 -> ... -> step_12
WITH step_01 AS (
    SELECT id, value FROM raw_data
),

step_02 AS (
    SELECT id, value + 1 AS value FROM step_01
),

step_03 AS (
    SELECT id, value + 1 AS value FROM step_02
),

step_04 AS (
    SELECT id, value + 1 AS value FROM step_03
),

step_05 AS (
    SELECT id, value + 1 AS value FROM step_04
),

step_06 AS (
    SELECT id, value + 1 AS value FROM step_05
),

step_07 AS (
    SELECT id, value + 1 AS value FROM step_06
),

step_08 AS (
    SELECT id, value + 1 AS value FROM step_07
),

step_09 AS (
    SELECT id, value + 1 AS value FROM step_08
),

step_10 AS (
    SELECT id, value + 1 AS value FROM step_09
),

step_11 AS (
    SELECT id, value + 1 AS value FROM step_10
),

step_12 AS (
    SELECT id, value + 1 AS value FROM step_11
)

SELECT * FROM step_12
