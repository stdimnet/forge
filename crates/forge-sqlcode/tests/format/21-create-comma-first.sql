-- @style commaFirst=true
create table t (a int, b text, primary key (a));
-- @expected
CREATE TABLE t (
    a INT
  , b TEXT
  , PRIMARY KEY (a)
);
