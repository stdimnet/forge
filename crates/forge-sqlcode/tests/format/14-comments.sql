-- leading comment
select a, /* inline */ b
from t -- trailing
where x = 1;


select 2; -- after
-- @expected
-- leading comment
SELECT a, /* inline */ b
FROM t -- trailing
WHERE x = 1;

SELECT 2; -- after
