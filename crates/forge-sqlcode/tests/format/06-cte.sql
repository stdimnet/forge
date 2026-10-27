with recent as (select * from orders where created_at > now() - interval '7 days'), big as (select user_id from recent where total > 100) select count(*) from big;
-- @expected
WITH recent AS (
    SELECT *
    FROM orders
    WHERE created_at > now() - INTERVAL '7 days'
), big AS (
    SELECT user_id
    FROM recent
    WHERE total > 100
)
SELECT count(*)
FROM big;
