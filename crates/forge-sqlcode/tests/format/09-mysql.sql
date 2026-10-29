-- @dialect mysql
select `id`, "text" from `users` # hash comment
where name = 'it\'s' limit 5, 10;
-- @expected
SELECT `id`, "text"
FROM `users` # hash comment
WHERE name = 'it\'s'
LIMIT 5, 10;
