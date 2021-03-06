use postgres::{Client, NoTls};
use chrono::{DateTime, Utc};
use std::time::Instant;

use std::collections::BinaryHeap;
use std::cmp::Reverse;
use std::convert::TryFrom;

// IO stuff
use std::io::prelude::*;

use crate::exchange::{Exchange, Market, Order, SecStat, Trade, UserAccount, OrderStatus};
use crate::account::AuthError;

use crate::buffer::{DatabaseReadyOrder};
/* ---- Specification for the db API ----
 *
 *      Functions that start with populate will read from the db on program startup ONLY.
 *          - These populate critical runtime data-structures.
 *      Functions that start with read will read from the db during normal program execution.
 *      Functions that start with write will write to the db during normal program execution.
 *
 *  If more than 1 operation occurs in a function, ex. deletes AND updates, this will be
 *  clearly described above the function.
 **/

/* Helper function for populate_exchange_markets.
 *
 * Directly inserts this order to the market
 * If the market didn't exist, we will return it as Some(Market)
 * so the calling function can add it to the exchange.
 */
fn direct_insert_to_market(potential_market: Option<&mut Market>, order: &Order) -> Option<Market> {
    // Get the market, or create it if it doesn't exist yet.
    match potential_market {
        Some(market) => {
            match &order.action[..] {
                "BUY" => {
                    market.buy_orders.push(order.clone());
                },
                "SELL" => {
                    market.sell_orders.push(Reverse(order.clone()));
                },
                _ => ()
            }
        },
        None => {
            // The market doesn't exist, create it.
            // buy is a max heap, sell is a min heap.
            let mut buy_heap: BinaryHeap<Order> = BinaryHeap::new();
            let mut sell_heap: BinaryHeap<Reverse<Order>> = BinaryHeap::new();

            // Store order on market, and in users account.
            match &order.action[..] {
                "BUY" => {
                    buy_heap.push(order.clone());
                },
                "SELL" => {
                    sell_heap.push(Reverse(order.clone()));
                },
                // We can never get here.
                _ => ()
            };

            // Create the new market
            let new_market = Market::new(buy_heap, sell_heap);
            return Some(new_market);
        }
    }
    return None;
}

/* Default initializes all entries in hashmap to false,
 * then sets the entries that have trades to true.
 **/
pub fn populate_has_trades(exchange: &mut Exchange, conn: &mut Client) {
    // 1. Read all markets in our exchange.
    let result = conn.query("SELECT symbol FROM Markets;", &[]);
    match result {
        Ok(rows) => {
            for row in rows {
                let symbol: &str = row.get(0);
                exchange.has_trades.insert(symbol.to_string().clone(), false);
            }
        },
        Err(e) => {
            eprintln!("{:?}", e);
            panic!("Query to read all market symbols failed!");
        }
    }
    // 2. Set markets with trades to true.
    let result = conn.query("SELECT DISTINCT symbol FROM ExecutedTrades;", &[]);
    match result {
        Ok(rows) => {
            for row in rows {
                let symbol: &str = row.get(0);
                exchange.has_trades.insert(symbol.to_string().clone(), true);
            }
        },
        Err(e) => {
            eprintln!("{:?}", e);
            panic!("Query to read all markets with trades failed!");
        }
    }
}

// TODO
/* Get the relevant pending orders from all
 * the markets, and insert them into the exchange.
 *
 *      - Future note: If we distribute markets across
 *        machines, it might be a good idea to provide
 *        a list of markets to read from.
 * */
pub fn populate_exchange_markets(exchange: &mut Exchange, conn: &mut Client) {
    // We order by symbol (market) and action, since this will probably increase cache hits.
    // This is because we populate the buys, then the sells, then move to the next market. High
    // spacial locality.
    for row in conn.query("\
SELECT o.* FROM PendingOrders p, Orders o
WHERE o.order_ID=p.order_ID;", &[]).expect("Something went wrong in the query.") {

        let order_id: i32 = row.get(0);
        let symbol: &str = row.get(1);
        let action: &str = row.get(2);
        let quantity: i32 = row.get(3);
        let filled: i32 = row.get(4);
        let price: f64 = row.get(5);
        let user_id: i32 = row.get(6);
        // No need to get status, it's obviously pending.

        let order = Order::direct(action, symbol, quantity, filled, price, order_id, OrderStatus::PENDING, user_id);
        // Add the order we found to the market.
        // If a new market was created, update the exchange.
        if let Some(market) = direct_insert_to_market(exchange.live_orders.get_mut(&order.symbol), &order) {
            exchange.live_orders.insert(order.symbol.clone(), market);
        };
    }
}

// TODO: Company Name??
/* Populate the statistics for each market
 *      - Future note: If we distribute markets across
 *        machines, it might be a good idea to provide
 *        a list of markets to read from.
 **/
pub fn populate_market_statistics(exchange: &mut Exchange, conn: &mut Client) {
    for row in conn.query("SELECT * FROM Markets", &[])
        .expect("Something went wrong in the query.") {

        let symbol: &str = row.get(0);
        // let company_name: &str = row.get(1);
        let total_buys: i32 = row.get(2);
        let total_sells: i32 = row.get(3);
        let filled_buys: i32 = row.get(4);
        let filled_sells: i32 = row.get(5);
        let latest_price: Option<f64> = row.get(6); // Price might be NULL if no trades occured.

        let market_stats = SecStat::direct(symbol, total_buys, total_sells, filled_buys, filled_sells, latest_price);
        exchange.statistics.insert(symbol.to_string().clone(), market_stats);
    }
}

// TODO
/* Populate the statistics of the exchange
 *      - Future note: If we distribute markets across
 *        machines, it might be a good idea to provide
 *        a list of markets to read from.
 **/
pub fn populate_exchange_statistics(exchange: &mut Exchange, conn: &mut Client) {
    for row in conn.query("SELECT total_orders FROM ExchangeStats", &[])
        .expect("Something went wrong in the query.") {

        let total_orders: Option<i32> = row.get(0);
        match total_orders {
            Some(count) => exchange.total_orders = count,
            None => exchange.total_orders = 0
        }
    }
}

/* Upgrade the database according to the config file.
 * TODO:
 *      When we fulfill a request, replace the first word with #
 *      as it can signify a comment/completed task.
 * */
pub fn upgrade_db<R>(reader: std::io::BufReader<R>, db_name: &String)
where
    R: std::io::Read
{
    let db_config = format!["host=localhost user=postgres dbname={}", db_name];
    let mut conn = Client::connect(db_config.as_str(), NoTls)
        .expect("Failed to connect to Database!");

    let mut query_string = String::from("\
INSERT INTO Markets
(symbol, name, total_buys, total_sells, filled_buys, filled_sells, latest_price)
Values
");
    for line in reader.lines() {
        match line {
            Ok(line) => {
                let mut components = line.split(',');
                let action = components.next().unwrap();
                let symbol = components.next().unwrap();
                let company_name = str::replace(components.next().unwrap(), "'", "''"); // sanitize input

                if action == "add" {
                    query_string.push_str(format!["('{}', '{}', 0, 0, 0, 0, NULL),\n", symbol, company_name].as_str());
                }
            },
            Err(e) => eprintln!("{}", e)
        }
    }

    query_string.pop(); // Removes newline
    query_string.pop(); // Removes last comma

    query_string.push(';');

    if let Err(e) = conn.query(query_string.as_str(), &[]) {
        eprintln!("{:?}", e);
        panic!("Query to upgrade database failed!");
    }

    println!("Upgrade complete!");

}

/* Reads total user count from database for new user IDs. */
pub fn read_total_accounts(conn: &mut Client) -> i32 {
    match conn.query("SELECT count(*) FROM Account;", &[]) {
        Ok(result) => {
            let row = &result[0];
            let count: i64 = row.get(0);
            return i32::try_from(count).unwrap();
        },
        Err(e) => {
            eprintln!("{}", e);
            panic!("Query to get total accounts number failed");
        }
    }
}

/* Check the database to see if the account user exists.  */
pub fn read_account_exists(username: &String, conn: &mut Client) -> bool {
    for row in conn.query("SELECT ID FROM Account WHERE Account.username = $1",
                          &[username]).expect("There was an issue while checking if the user is in the database.") {

        let id: Option<i32> = row.get(0);
        if let Some(_) = id {
            return true;
        }
    }
    return false;
}

/* Compare the provided username + password combo against the database.
 * If they match, return the UserAccount, otherwise, return the error that occurred.
 **/
pub fn read_auth_user<'a>(username: &'a String, password: &String, conn: &mut Client) -> Result<UserAccount, AuthError<'a>> {
    let query_string = "SELECT ID, username, password FROM Account WHERE Account.username = $1";
    match conn.query(query_string, &[&username]) {
        Ok(result) => {
            // Did not find the user
            if result.len() == 0 {
                return Err(AuthError::NoUser(username));
            }

            // Found a user, usernames are unique so we get 1 row.
            let row = &result[0];
            let recv_id: i32 = row.get(0);
            let recv_username: &str = row.get(1);
            let recv_password: &str = row.get(2);

            // User authenticated.
            if *password == recv_password {
                return Ok(UserAccount::direct(recv_id, recv_username, recv_password));
            }

            // Password was incorrect.
            return Err(AuthError::BadPassword(None));
        },
        Err(e) => {
            eprintln!("{}", e);
            panic!("Something went wrong with the authenticate query!");
        }
    }

}

/* Read the account with the given username and return the account. */
pub fn read_account(username: &String, conn: &mut Client) -> Result<UserAccount, postgres::error::Error> {
    match conn.query("SELECT ID, username, password FROM Account where Account.username = $1", &[username]) {
        Ok(result) => {
            let row = &result[0];
            let recv_id: i32 = row.get(0);
            let recv_username: &str = row.get(1);
            let recv_password: &str = row.get(2);

            return Ok(UserAccount::direct(recv_id, recv_username, recv_password));
        },
        Err(e) => {
            eprintln!("{}", e);
            return Err(e);
        }
    }
}

/* Read the account with the given user ID and return the username. */
pub fn read_user_by_id(id: i32, conn: &mut Client) -> Result<String, postgres::error::Error> {
    match conn.query("SELECT username FROM Account where Account.id = $1", &[&id]) {
        Ok(result) => {
            let row = &result[0];
            let recv_username: &str = row.get(0);

            return Ok(recv_username.to_string());
        },
        Err(e) => {
            eprintln!("{}", e);
            return Err(e);
        }
    }
}

/* Read the pending orders that belong to this user into their account.
 * This is currently not in use, however, if we only store a subset
 * of market info in the in-mem markets, we will have to call this
 * to get the full view of an account (in, say, print_user).
 **/
pub fn read_account_pending_orders(user: &mut UserAccount, conn: &mut Client) {
    let query_string = "\
SELECT (o.order_ID, o.symbol, o.action, o.quantity, o.filled, o.price, o.user_ID) FROM Orders o, PendingOrders p
WHERE o.order_ID = p.order_ID
AND o.user_ID =
    (SELECT ID FROM Account WHERE Account.username = $1)
ORDER BY o.order_ID;";
    for row in conn.query(query_string, &[&user.username]).expect("Query to fetch pending orders failed!") {
        let order_id:       i32  = row.get(0);
        let symbol:         &str = row.get(1);
        let action:         &str = row.get(2);
        let quantity:       i32  = row.get(3);
        let filled:         i32  = row.get(4);
        let price:          f64  = row.get(5);
        let user_id:        i32  = row.get(6);
        // let time_placed:    i32  = row.get(8); // <---- TODO
        // let time_updated:   i32  = row.get(9); // <---- TODO

        // We will just re-insert everything.
        let order = Order::direct(action,
                                  symbol,
                                  quantity,
                                  filled,
                                  price,
                                  order_id,
                                  OrderStatus::PENDING,
                                  user_id);

        user.pending_orders.insert_order(order);
    }
}


/* Get this accounts executed trades from the database. */
pub fn read_account_executed_trades(user: &UserAccount, executed_trades: &mut Vec<Trade>, conn: &mut Client) {
    // First, lets get trades where we had our order filled.
    let query_string = "\
SELECT * FROM ExecutedTrades e
WHERE
e.filled_UID = (SELECT ID FROM Account WHERE Account.username = $1) OR
e.filler_UID = (SELECT ID FROM Account WHERE Account.username = $1)
ORDER BY e.execution_time;";

    for row in conn.query(query_string, &[&user.username]).expect("Query to fetch executed trades failed!") {
        let symbol:     &str = row.get(0);
        let mut action: &str = row.get(1);
        let price:      f64  = row.get(2);
        let filled_oid: i32  = row.get(3);
        let filled_uid: i32  = row.get(4);
        let filler_oid: i32  = row.get(5);
        let filler_uid: i32  = row.get(6);
        let exchanged:  i32  = row.get(7);
        let execution_time:
            DateTime<Utc>    = row.get(8);

        // Switch the action because we were the filler.
        if user.id.unwrap() == filler_uid {
            match action {
                "BUY" => action = "SELL",
                "SELL" => action = "BUY",
                _ => ()
            }
        }

        let trade = Trade::direct(symbol,
                                  action,
                                  price,
                                  filled_oid,
                                  filled_uid,
                                  filler_oid,
                                  filler_uid,
                                  exchanged,
                                  execution_time);
        executed_trades.push(trade);
    }
}

/* TODO: Accept time periods!
 * Read past trades for the requested security from the database.
 * Returns Some(Vec<Trade>) if there are trades,
 * otherwise, returns None.
 **/
pub fn read_trades(symbol: &String, conn: &mut Client) -> Option<Vec<Trade>> {
    let mut trades: Vec<Trade> = Vec::new();
    for row in conn.query("SELECT * FROM ExecutedTrades WHERE symbol=$1",
                          &[&symbol.as_str()]).expect("Read Trades query (History) failed!") {

        let symbol:     &str = row.get(0);
        let action:     &str = row.get(1);
        let price:      f64  = row.get(2);
        let filled_oid: i32  = row.get(3);
        let filled_uid: i32  = row.get(4);
        let filler_oid: i32  = row.get(5);
        let filler_uid: i32  = row.get(6);
        let exchanged:  i32  = row.get(7);
        let execution_time:
            DateTime<Utc>    = row.get(8);

        trades.push(Trade::direct(symbol,
                                  action,
                                  price,
                                  filled_oid,
                                  filled_uid,
                                  filler_oid,
                                  filler_uid,
                                  exchanged,
                                  execution_time
                                 ));
    }
    return Some(trades);
}

/* TODO: Doesn't get called ever, since we have a perfect market view.
 *       If we cap the number of orders visible to a market in the program,
 *       keeping the rest in the DB, then we may trigger this code.
 *
 * Returns Some(action) if the user owns this pending order, else None. */
pub fn read_match_pending_order(user_id: i32, order_id: i32, conn: &mut Client) -> Option<String> {
    let result = conn.query("\
SELECT action
FROM Orders o, PendingOrders p
WHERE p.order_id = $1
  AND o.order_id = p.order_id
  AND o.user_id  = $2;", &[&order_id, &user_id]);

    match result {
        Ok(rows) => {
            if rows.len() == 1 {
                for row in rows {
                    let action: &str = row.get(0);
                    return Some(action.to_string().clone());
                }
            }
        },
        Err(e) => {
            eprintln!("{:?}", e);
            panic!("Match pending order query failed!");
        }
    }
    return None;
}

/* TODO: Prepared statement.
 * Write a new user to the database. */
pub fn write_insert_new_account(account: &UserAccount, conn: &mut Client) -> Result<(), ()> {
    let now = Utc::now();

    let query_string = "INSERT INTO Account (ID, username, password, register_time) VALUES ($1, $2, $3, $4);";
    match conn.execute(query_string, &[&account.id.unwrap(), &account.username, &account.password, &now]) {
        Ok(_) => return Ok(()),
        Err(e) => {
            eprintln!("{:?}", e);
            return Err(());
        }
    }
}


/* Returns true if the market exists in our database, false otherwise. */
pub fn read_market_exists(market: &String, conn: &mut Client) -> bool {
    let query_string = "SELECT symbol from Markets where symbol=$1;";
    match conn.query(query_string, &[market]) {
        Ok(result) => {
            if result.len() == 1 {
                return true;
            }
        },
        Err(e) => {
            eprintln!("{}", e);
            panic!("Something went wrong while querying the database for the market symbol.");
        }
    }

    return false;
}

/* Reads the first `n` market symbols into the symbol_vec Vector.
 * `n` is described by the capacity of symbol_vec.
 *
 * This can *almost* be thought of as a 'populate' function, however
 * we need to call it each time we run a simulation.
 */
pub fn read_exchange_markets_simulations(symbol_vec: &mut Vec<String>, conn: &mut Client) {
    let mut i = 0;
    let limit = symbol_vec.capacity();
    for row in conn.query("SELECT symbol FROM Markets;", &[])
        .expect("Something went wrong in the query.") {

        let symbol: &str = row.get(0);
        symbol_vec.push(symbol.to_string());
        i += 1;
        if i == limit {
            return;
        }
    }
}


/******************************************************************************************************
 *                                         Buffered Writes API                                        *
 ******************************************************************************************************/
/* TODO: Multi-row updates if possible.
 **/
pub fn insert_buffered_orders(orders: &Vec<DatabaseReadyOrder>, conn: &mut Client) {

    let start = Instant::now();
    // TIMING
    let query_exec_time = Instant::now();
    let mut transaction = conn.transaction().expect("Failed to initiate transaction!");

    // Everything is to be updated
    let query_string = "\
INSERT INTO Orders
(order_ID, symbol, action, quantity, filled, price, user_ID, status, time_placed, time_updated)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10);";

    let statement = match transaction.prepare(&query_string) {
        Ok(stmt) => stmt,
        Err(e) => {
            eprintln!("{}", e);
            panic!("Failed to insert new orders to database!");
        }
    };

    for order in orders {

        let status: String = format!["{:?}", order.status.unwrap()];

        transaction.execute(&statement, &[ &order.order_id,
                                          &order.symbol,
                                          &order.action,
                                          &order.quantity,
                                          &order.filled,
                                          &order.price,
                                          &order.user_id,
                                          &status,
                                          &order.time_placed,
                                          &order.time_updated
                                         ]).expect("FAILED TO EXEC INSERT ORDERS");
    }

    transaction.commit().expect("Failed to commit buffered order insert transaction.");
    let query_exec_time = query_exec_time.elapsed().as_millis();
    let end = start.elapsed().as_millis();
    println!("
Insert New Order Speed
\tQuery Build Time Elapsed: DNE
\tQuery Exec Time Elapsed: {} ms
\tTotal Time Elapsed: {} ms
\tTotal Items Inserted: {}
", query_exec_time, end, orders.len());

}


/* TODO: Multi-row updates if possible.
 **/
pub fn update_buffered_orders(orders: &Vec<DatabaseReadyOrder>, conn: &mut Client) {

    let start = Instant::now();

    // 3 types of updates
    // 1. filled & time updated
    // 2. status & time updated
    // 3. filled & status & time updated
    let filled_string = "UPDATE Orders SET filled=$1, time_updated=$2 WHERE order_id=$3;";
    let status_string = "UPDATE Orders SET status=$1, time_updated=$2 WHERE order_id=$3;";
    let total_string = "UPDATE Orders SET filled=$1, status=$2, time_updated=$3 WHERE order_id=$4;";


    // TIMING
    let query_exec_time = Instant::now();
    let mut transaction = conn.transaction().expect("Failed to initiate transaction!");

    let filled_stmt = match transaction.prepare(&filled_string) {
        Ok(stmt) => stmt,
        Err(e) => {
            eprintln!("{}", e);
            panic!("Failed to create 'filled' prepared statement for updated orders!");
        }
    };
    let status_stmt = match transaction.prepare(&status_string) {
        Ok(stmt) => stmt,
        Err(e) => {
            eprintln!("{}", e);
            panic!("Failed to create 'status' prepared statement for updated orders!");
        }
    };
    let total_stmt = match transaction.prepare(&total_string) {
        Ok(stmt) => stmt,
        Err(e) => {
            eprintln!("{}", e);
            panic!("Failed to create 'total' prepared statement for updated orders!");
        }
    };

    enum UpdateType {
        FILL,
        STATUS,
        TOTAL,
        NONE
    }

    for order in orders {
        let mut order_type = UpdateType::NONE;

        let mut filled: Option<i32> = None;
        let mut status: Option<String> = None;

        if let Some(amount_filled) = order.filled {
            filled = Some(amount_filled);
            order_type = UpdateType::FILL;
        }

        if let Some(new_status) = order.status {
            status = Some(format!["{:?}", new_status]);
            if let UpdateType::FILL = order_type {
                order_type = UpdateType::TOTAL;
            } else {
                order_type = UpdateType::STATUS;
            }
        }

        if let Some(update_time) = order.time_updated {
            let time_updated = update_time;

            match order_type {
                UpdateType::FILL => {
                    let filled = filled.unwrap();
                    if let Err(e) = transaction.execute(&filled_stmt, &[&filled, &time_updated, &order.order_id.unwrap()]) {
                        eprintln!("{}", e);
                        panic!("Something went wrong with the buffered order update statement.");
                    }
                },
                UpdateType::STATUS => {
                    let status = status.unwrap();
                    if let Err(e) = transaction.execute(&status_stmt, &[&status, &time_updated, &order.order_id.unwrap()]) {
                        eprintln!("{}", e);
                        panic!("Something went wrong with the buffered order update statement.");
                    }
                },
                UpdateType::TOTAL => {
                    let filled = filled.unwrap();
                    let status = status.unwrap();
                    if let Err(e) = transaction.execute(&total_stmt, &[&filled, &status, &time_updated, &order.order_id.unwrap()]) {
                        eprintln!("{}", e);
                        panic!("Something went wrong with the buffered order update statement.");
                    }
                },
                UpdateType::NONE => panic!("Our updated order has no data??")
            }
        };
    }
    transaction.commit().expect("Failed to commit buffered order update transaction.");
    let query_exec_time = query_exec_time.elapsed().as_millis();
    let end = start.elapsed().as_millis();
    println!("\
Update Known Order Speed
\tQuery Build Time Elapsed: DNE
\tQuery Exec Time Elapsed: {} ms
\tTotal Time Elapsed: {} ms
\tTotal Items Updated: {}
", query_exec_time, end, orders.len());
}


/* Performs 1 or more multi-row inserts to the pending orders table in
 * a single transaction. */
pub fn insert_buffered_pending(pending: &Vec<i32>, conn: &mut Client) {
    // TIMING
    let start = Instant::now();

    let mut queries: Vec<String> = Vec::new();
    let query_string = String::from("INSERT INTO PendingOrders (order_id) VALUES ");
    queries.push(query_string.clone());

    let mut counter = 0;
    let cap = 100000; // Number of rows per statement
    let mut index = 0;

    // TIMING
    let query_build_time = Instant::now();

    for order in pending {
        if counter < cap {
            queries[index].push_str(&format!["({}),\n", order].as_str());
        } else {
            // 1. Terminate the current query
            queries[index].pop();
            queries[index].pop();
            queries[index].push(';');
            // 2 Update counters
            index += 1;
            counter = 0;
            // 3. Start new query
            queries.push(query_string.clone());
            queries[index].push_str(&format!["({}),\n", order].as_str());
        }
        counter += 1;
    }

    queries[index].pop();
    queries[index].pop();
    queries[index].push(';');

    let query_build_time = query_build_time.elapsed().as_millis();

    let query_exec_time = Instant::now();
    let mut transaction = conn.transaction().expect("Failed to initiate transaction!");
    // If we have a statement to execute...
    if (counter != 0) || (queries.len() > 1) {
        // Execute all the queries.
        for query in &queries {
            if let Err(e) = transaction.execute(query.as_str(), &[]) {
                eprintln!("{}", e);
                eprintln!("{}", query);
                panic!("Failed to exec insert PendingOrders.");
            }
        }
    }
    transaction.commit().expect("Failed to commit buffered pending order insert transaction.");
    let query_exec_time = query_exec_time.elapsed().as_millis();
    let end = start.elapsed().as_millis();

    println!("\
Insert Pending Speed (n = 100000)
\tQuery Build Time Elapsed: {} ms
\tQuery Exec Time Elapsed: {} ms
\tTotal Time Elapsed: {} ms
\tTotal Items Inserted: {}
", query_build_time, query_exec_time, end, pending.len());
}


/* Performs 1 or more multi-row delete queries to the pending orders table
 * in a single transaction. */
pub fn delete_buffered_pending(pending: &Vec<i32>, conn: &mut Client) {
    let start = Instant::now();

    // TIMING
    let query_build_time = Instant::now();
    let mut queries: Vec<String> = Vec::new();
    let query_string = String::from("DELETE FROM PendingOrders WHERE order_id IN ( ");
    queries.push(query_string.clone());

    let mut counter = 0;
    let cap = 100000; // Number of rows per statement
    let mut index = 0;

    for order in pending {
        if counter < cap {
            queries[index].push_str(&format!["{}, ", order].as_str());
        } else {
            // 1. Terminate the current query
            queries[index].pop();
            queries[index].pop();
            queries[index].push_str(");");
            // 2 Update counters
            index += 1;
            counter = 0;
            // 3. Start new query
            queries.push(query_string.clone());
            queries[index].push_str(&format!["{}, ", order].as_str());
        }
        counter += 1;
    }

    queries[index].pop();
    queries[index].pop();
    queries[index].push_str(");");
    let query_build_time = query_build_time.elapsed().as_millis();

    let query_exec_time = Instant::now();
    let mut transaction = conn.transaction().expect("Failed to initiate transaction!");

    // If we have a statement to execute...
    if (counter != 0) || (queries.len() > 1) {
        for query in &queries {
            if let Err(e) = transaction.execute(query.as_str(), &[]) {
                eprintln!("{}", e);
                eprintln!("{}", query);
                panic!("Failed to exec delete pending query.");
            }
        }
    }
    transaction.commit().expect("Failed to commit buffered pending order delete transaction.");

    let query_exec_time = query_exec_time.elapsed().as_millis();
    let end = start.elapsed().as_millis();

    println!("\
Delete Pending Speed (n = 100000)
\tQuery Build Time Elapsed: {} ms
\tQuery Exec Time Elapsed: {} ms
\tTotal Time Elapsed: {} ms
\tTotal Items Deleted: {}
", query_build_time, query_exec_time, end, pending.len());

}


/* A single query to set or update the exchange stats. */
pub fn update_total_orders(total_orders: i32, conn: &mut Client) {
    let mut transaction = conn.transaction().expect("Failed to initiate transaction!");
    // Update the exchange total orders
    let query_string = "\
INSERT INTO ExchangeStats
VALUES (1, $1)
ON CONFLICT (key) DO
UPDATE SET total_orders=$1;";

    if let Err(e) = transaction.execute(query_string, &[&total_orders]) {
        eprintln!("{:?}", e);
        panic!("Something went wrong with the exchange total orders update query!");
    };

    transaction.commit().expect("Failed to commit buffered total order update transaction.");
}


/* Performs an update per modified market in a single transaction.
 * This table always stays small, so the cost of 1 connection per
 * query is negligible, especially when using a prepared statement.
 **/
pub fn update_buffered_markets(markets: &Vec<SecStat>, conn: &mut Client) {
    let mut transaction = conn.transaction().expect("Failed to initiate transaction!");
    let query_string = "\
UPDATE Markets
SET (total_buys, total_sells, filled_buys, filled_sells, latest_price) =
($1, $2, $3, $4, $5)
WHERE Markets.symbol = $6;";

    let statement = match transaction.prepare(&query_string) {
        Ok(stmt) => stmt,
        Err(e) => {
            eprintln!("{}", e);
            panic!("Failed to insert new orders to database!");
        }
    };

    for market in markets {
        transaction.execute(&statement, &[  &market.total_buys,
                                            &market.total_sells,
                                            &market.filled_buys,
                                            &market.filled_sells,
                                            &market.last_price,
                                            &market.symbol
                                         ]).expect("FAILED TO EXEC UPDATE MARKETS");
    }
    transaction.commit().expect("Failed to commit buffered market update transaction.");
}


/* Performs 1 or more multi-row inserts in a single transaction. */
pub fn insert_buffered_trades(trades: &Vec<Trade>, conn: &mut Client) {
    // TIMING
    let start = Instant::now();

    let mut queries: Vec<String> = Vec::new();
    let query_string = String::from("INSERT INTO ExecutedTrades
(symbol, action, price, filled_OID, filled_UID, filler_OID, filler_UID, exchanged, execution_time)
VALUES ");
    queries.push(query_string.clone());

    let mut counter = 0;
    let cap = 100000; // Number of rows per statement
    let mut index = 0;

    // TIMING
    let query_build_time = Instant::now();

    for trade in trades {
        if counter < cap {
            queries[index].push_str(&format!["('{}', '{}', {}, {}, {}, {}, {}, {}, '{}'),\n",  trade.symbol,
                                                                                               trade.action,
                                                                                               trade.price,
                                                                                               trade.filled_oid,
                                                                                               trade.filled_uid,
                                                                                               trade.filler_oid,
                                                                                               trade.filler_uid,
                                                                                               trade.exchanged,
                                                                                               trade.execution_time.to_rfc3339()].as_str());
        } else {
            // 1. Terminate the current query
            queries[index].pop();
            queries[index].pop();
            queries[index].push(';');
            // 2 Update counters
            index += 1;
            counter = 0;
            // 3. Start new query
            queries.push(query_string.clone());
            queries[index].push_str(&format!["('{}', '{}', {}, {}, {}, {}, {}, {}, '{}'),\n",  trade.symbol,
                                                                                               trade.action,
                                                                                               trade.price,
                                                                                               trade.filled_oid,
                                                                                               trade.filled_uid,
                                                                                               trade.filler_oid,
                                                                                               trade.filler_uid,
                                                                                               trade.exchanged,
                                                                                               trade.execution_time.to_rfc3339()].as_str());
        }
        counter += 1;
    }

    queries[index].pop();
    queries[index].pop();
    queries[index].push(';');

    // TIMING
    let query_build_time = query_build_time.elapsed().as_millis();

    let query_exec_time = Instant::now();
    let mut transaction = conn.transaction().expect("Failed to initiate transaction!");

    // If we have a statement to execute...
    if (counter != 0) || (queries.len() > 1) {
        for query in &queries {
            if let Err(e) = transaction.execute(query.as_str(), &[]) {
                eprintln!("{}", e);
                eprintln!("{}", query);
                panic!("Failed to exec insert ExecutedTrades.");

            }
        }
    }
    transaction.commit().expect("Failed to commit buffered trade insert transaction.");

    let query_exec_time = query_exec_time.elapsed().as_millis();
    let end = start.elapsed().as_millis();
    println!("\
Insert Executed Trade Speed (n = 100000)
\tQuery Build Time Elapsed: {} ms
\tQuery Exec Time Elapsed: {} ms
\tTotal Time Elapsed: {} ms
\tTotal Items Inserted: {}
", query_build_time, query_exec_time, end, trades.len());

}

