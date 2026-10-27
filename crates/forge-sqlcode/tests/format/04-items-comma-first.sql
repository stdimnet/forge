-- @style itemsNewline=true commaFirst=true
select id, name, email from users group by id, name order by id desc, name
-- @expected
SELECT
    id
  , name
  , email
FROM users
GROUP BY
    id
  , name
ORDER BY
    id DESC
  , name
