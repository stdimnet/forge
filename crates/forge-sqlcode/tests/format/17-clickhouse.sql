-- @dialect clickhouse
select count() from hits prewhere CounterID = 34 where EventDate >= today() - 7 group by UserID settings max_threads = 4 format JSON
-- @expected
SELECT count()
FROM hits
PREWHERE CounterID = 34
WHERE EventDate >= today() - 7
GROUP BY UserID
SETTINGS max_threads = 4
FORMAT JSON
