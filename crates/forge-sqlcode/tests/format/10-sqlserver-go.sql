-- @dialect sqlserver
select top 10 [order id] from [dbo].[orders] where [status]=N'paid'
go
select 1
GO
-- @expected
SELECT TOP 10 [order id]
FROM [dbo].[orders]
WHERE [status] = N'paid'
GO
SELECT 1
GO
