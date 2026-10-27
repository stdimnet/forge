-- @style keywordCase=lower identifierCase=upper
SELECT Id, "Name" FROM Users WHERE Status = 'Active' -- Keep Me
-- @expected
select ID, "Name"
from USERS
where STATUS = 'Active' -- Keep Me
