-- @dialect postgres
create or replace function total_of(uid int) returns numeric as $$
  select   sum(total) from orders where user_id = uid;
$$ language sql;
select total_of(1)::text, data->>'name' from users;
-- @expected
CREATE OR REPLACE FUNCTION total_of(uid INT) returns NUMERIC AS $$
  select   sum(total) from orders where user_id = uid;
$$ language sql;
SELECT total_of(1)::TEXT, data ->> 'name'
FROM users;
