select id,name from users where active=true and created_at>'2024-01-01' order by name asc limit 10;
-- @expected
SELECT id, name
FROM users
WHERE active = TRUE
    AND created_at > '2024-01-01'
ORDER BY name ASC
LIMIT 10;
