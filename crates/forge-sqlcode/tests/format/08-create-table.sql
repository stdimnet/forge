create table if not exists orders (id bigserial primary key, user_id integer not null references users(id), total numeric(10,2) default 0, created_at timestamp default now());
-- @expected
CREATE TABLE IF NOT EXISTS orders (
    id BIGSERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users (id),
    total NUMERIC(10, 2) DEFAULT 0,
    created_at TIMESTAMP DEFAULT now()
);
