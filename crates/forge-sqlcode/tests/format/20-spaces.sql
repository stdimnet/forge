-- @style spaceAroundOperators=false spaceAfterComma=false spaceWithinParentheses=true blankLinesBetween=0
select a, b from t where x = coalesce(y, 1);


select 2;
-- @expected
SELECT a,b
FROM t
WHERE x=coalesce( y,1 );
SELECT 2;
