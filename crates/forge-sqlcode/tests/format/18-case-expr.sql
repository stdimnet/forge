select case when a > 1 then 'big' when a = 1 then 'one' else 'small' end as size, -a, a * 2, count(*), t.* from t
-- @expected
SELECT CASE WHEN a > 1 THEN 'big' WHEN a = 1 THEN 'one' ELSE 'small' END AS size, -a, a * 2, count(*), t.*
FROM t
