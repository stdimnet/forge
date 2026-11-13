select * from (select user_id, sum(total) total from orders group by user_id) t where t.total > (select avg(total) from orders);
-- @expected
SELECT *
FROM (
    SELECT user_id, sum(total) total
    FROM orders
    GROUP BY user_id
) t
WHERE t.total > (
    SELECT avg(total)
    FROM orders
);
