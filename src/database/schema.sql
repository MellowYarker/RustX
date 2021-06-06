CREATE TABLE Account (
    ID              int,
    username        varchar(15) NOT NULL,
    password        varchar(20) NOT NULL,
    register_time   date,
    PRIMARY KEY(ID)
);

CREATE TABLE Orders (
    order_ID        int,
    symbol          varchar(4) NOT NULL,
    action          varchar(4) NOT NULL,
    quantity        int,
    filled          int,
    price           float8,
    user_ID         int,
    status          varchar(8) NOT NULL,
    time_placed     date,
    time_updated    date,
    PRIMARY KEY(order_ID),
    /* CONSTRAINT fk_account */
        FOREIGN KEY(user_ID)
            REFERENCES Account(ID)
);


CREATE TABLE PendingOrders (
    order_ID        int,
    PRIMARY KEY(order_ID),
    /* CONSTRAINT fk_order */
        FOREIGN KEY(order_ID)
            REFERENCES Orders(order_ID)
);

CREATE TABLE ExecutedTrades (
    symbol          varchar(4) NOT NULL,
    action          varchar(4) NOT NULL,
    price           float8,
    filled_OID      int,
    filled_UID      int,
    filler_OID      int,
    filler_UID      int,
    exchanged       int,
    execution_time  date,
    -- Will never have 2+ trades with the same
    -- (filled, filler) order id pair
    PRIMARY KEY(filled_OID, filler_OID),
    /* CONSTRAINT fk_Order_filled */
        FOREIGN KEY(filled_OID)
            REFERENCES Orders(order_ID),
    /* CONSTRAINT fk_Order_filler */
        FOREIGN KEY(filler_OID)
            REFERENCES Orders(order_ID),
    /* CONSTRAINT fk_Account_filled */
        FOREIGN KEY(filled_UID)
            REFERENCES Account(ID),
    /* CONSTRAINT fk_Account_filler */
        FOREIGN KEY(filler_UID)
            REFERENCES Account(ID)
);

CREATE TABLE Markets (
    symbol          varchar(4) NOT NULL,
    name            varchar(80) NOT NULL,
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
CREATE TABLE Exchange_Stats (
    total_orders    int
);
