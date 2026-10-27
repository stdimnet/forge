-- @style clauseNewline=false andOrNewline=false
select a,b from t where x=1 and y=2 order by a;
-- @expected
SELECT a, b FROM t WHERE x = 1 AND y = 2 ORDER BY a;
