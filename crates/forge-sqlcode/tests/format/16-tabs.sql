-- @style useTabs=true indent=4
select * from t where a = 1 and exists (select 1 from u where u.id = t.id)
-- @expected
SELECT *
FROM t
WHERE a = 1
	AND EXISTS (
		SELECT 1
		FROM u
		WHERE u.id = t.id
	)
