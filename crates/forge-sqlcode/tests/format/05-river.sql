-- @style alignRight=true itemsNewline=true
select id, name from users u join orders o on o.user_id = u.id where u.active and o.total > 10 order by id
-- @expected
SELECT
       id,
       name
  FROM users u
  JOIN orders o ON o.user_id = u.id
 WHERE u.active
   AND o.total > 10
 ORDER BY
       id
