select id from a union all select id from b union select id from c order by 1
-- @expected
SELECT id
FROM a
UNION ALL
SELECT id
FROM b
UNION
SELECT id
FROM c
ORDER BY 1
