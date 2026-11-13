select u.name, o.total from users u join orders o on o.user_id=u.id and o.status='paid' left outer join order_items i on i.order_id=o.id where u.id between 1 and 10 or u.name like 'a%';
-- @expected
SELECT u.name, o.total
FROM users u
JOIN orders o ON o.user_id = u.id
    AND o.status = 'paid'
LEFT OUTER JOIN order_items i ON i.order_id = o.id
WHERE u.id BETWEEN 1 AND 10
    OR u.name LIKE 'a%';
