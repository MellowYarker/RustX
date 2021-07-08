CREATE TABLE Account (
    ID              int,
    username        varchar(15) NOT NULL,
    password        varchar(20) NOT NULL,
    register_time   TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY(ID)
);

CREATE TABLE Orders (
    order_ID        int,
    symbol          varchar(10) NOT NULL,
    action          varchar(4) NOT NULL,
    quantity        int,
    filled          int,
    price           float8,
    user_ID         int,
    status          varchar(9) NOT NULL,
    time_placed     TIMESTAMP WITH TIME ZONE,
    time_updated    TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY(order_ID),
    FOREIGN KEY(user_ID)
        REFERENCES Account(ID)
);

-- This allows us to do HOT updates, critical for fast updates!
-- HOT (Heap only Tuple) updates do not force re-indexing + can
-- allow deletion of stale records outside of AUTOVACUUM Calls.
ALTER TABLE Orders SET (fillfactor = 70);

CREATE TABLE PendingOrders (
    order_ID        int,
    PRIMARY KEY(order_ID),
    FOREIGN KEY(order_ID)
        REFERENCES Orders(order_ID)
);

CREATE TABLE ExecutedTrades (
    symbol          varchar(10) NOT NULL,
    action          varchar(4) NOT NULL,
    price           float8,
    filled_OID      int,
    filled_UID      int,
    filler_OID      int,
    filler_UID      int,
    exchanged       int,
    execution_time  TIMESTAMP WITH TIME ZONE,
    -- Will never have 2+ trades with the same
    -- (filled, filler) order id pair
    PRIMARY KEY(filled_OID, filler_OID),
    FOREIGN KEY(filled_OID)
        REFERENCES Orders(order_ID),
    FOREIGN KEY(filler_OID)
        REFERENCES Orders(order_ID),
    FOREIGN KEY(filled_UID)
        REFERENCES Account(ID),
    FOREIGN KEY(filler_UID)
        REFERENCES Account(ID)
);

CREATE TABLE Markets (
    symbol          varchar(10) NOT NULL,
    name            varchar(300) NOT NULL,
    total_buys      int,
    total_sells     int,
    filled_buys     int,
    filled_sells    int,
    latest_price    float8,
    PRIMARY KEY(symbol)
);

-- We have to store the count in a table because postgresql
-- doesn't store row count as metadata, getting row count
-- with SELECT count(*) from Orders; would be prohibitively expensive
CREATE TABLE ExchangeStats (
    key             int,
    total_orders    int,
    PRIMARY KEY (key)
);
