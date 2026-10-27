-- @dialect postgres
insert into users(email,name) values ('a@b.c','A'),('d@e.f','D') on conflict (email) do update set name=excluded.name returning id;
update users set name='x', active=false where id=1;
delete from users where id in (1,2,3);
-- @expected
INSERT INTO users (email, name)
VALUES ('a@b.c', 'A'), ('d@e.f', 'D')
ON CONFLICT (email) DO UPDATE SET name = excluded.name
RETURNING id;
UPDATE users
SET name = 'x', active = FALSE
WHERE id = 1;
DELETE FROM users
WHERE id IN (1, 2, 3);
