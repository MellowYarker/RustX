INSERT INTO Account VALUES (1, 'milan', 'password', '2020-06-03');
INSERT INTO Account VALUES (2, 'red', 'password', '2020-06-03');
INSERT INTO Account VALUES (3, 'owen', 'password', '2020-06-03');
INSERT INTO Account VALUES (4, 'cass', 'password', '2020-06-03');
---
-- 3 initial orders
/* INSERT INTO Orders VALUES (1, 'PLTR', 'BUY', 10, 4, 25.00, 3, 'PENDING','2020-06-03', '2020-06-03'); */
INSERT INTO Orders VALUES (1, 'PLTR', 'BUY', 10, 0, 25.00, 3, 'PENDING','2020-06-03', '2020-06-03');
INSERT INTO Orders VALUES (2, 'MP', 'SELL', 7, 0, 32.00, 1, 'PENDING', '2020-06-03', '2020-06-03');
/* INSERT INTO Orders VALUES (3, 'DM', 'BUY', 18, 2, 14.00, 1, 'PENDING', '2020-06-03', '2020-06-03'); */
INSERT INTO Orders VALUES (3, 'DM', 'BUY', 18, 0, 14.00, 1, 'PENDING', '2020-06-03', '2020-06-03');
---
--- add the 3 to pending
INSERT INTO PendingOrders VALUES (1);
INSERT INTO PendingOrders VALUES (2);
INSERT INTO PendingOrders VALUES (3);
---
--- 2 new orders that filled stuff
INSERT INTO Orders VALUES (4, 'DM', 'SELL', 2, 2, 14.00, 2, 'COMPLETE', '2020-06-03', '2020-06-03');
INSERT INTO Orders VALUES (5, 'PLTR', 'SELL', 4, 4, 25.00, 4, 'COMPLETE', '2020-06-03', '2020-06-03');
---
--- Update the old pending orders
UPDATE Orders
SET filled=4
WHERE order_id=1;

UPDATE Orders
SET filled=2
WHERE order_id=3;
---
-- Move the trades into ExecutedTrades
INSERT INTO ExecutedTrades VALUES ('DM', 'BUY', 14.00, 3, 1, 4, 2, 2, '2020-06-03');
INSERT INTO ExecutedTrades VALUES ('PLTR', 'BUY', 25.00, 1, 3, 5, 4, 4, '2020-06-03');
---
-- we have 5 orders.
INSERT INTO ExchangeStats VALUES(5);
---
INSERT INTO Markets VALUES('PLTR', 'Palantir Technologies', 1, 1, 1, 1, 25.00);
INSERT INTO Markets VALUES('MP', 'MP Materials', 0, 1, 0, 0, NULL);
INSERT INTO Markets VALUES('DM', 'Desktop Metal Inc.', 1, 1, 1, 1, 14.00);
