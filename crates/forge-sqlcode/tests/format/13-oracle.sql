-- @dialect oracle
select * from emp where rownum<=5
/
begin
  null;
end;
/
-- @expected
SELECT *
FROM emp
WHERE ROWNUM <= 5
/
BEGIN
  NULL;
END;
/
