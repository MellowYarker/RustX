use postgres::Client;
use std::collections::BinaryHeap;
use std::cmp::Reverse;

use crate::exchange::{Exchange, Market, Order, SecStat, Trade};

// Directly inserts this order to the market
// If the market didn't exist, we will return it as Some(Market)
// so the calling function can add it to the exchange.
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
    for row in conn.query("SELECT o.* FROM PendingOrders p, Orders o WHERE o.order_ID=p.order_ID ORDER BY (o.symbol, o.action)", &[])
        .expect("Something went wrong in the query.") {

        let order_id: i32 = row.get(0);
        let symbol: &str = row.get(1);
        let action: &str = row.get(2);
        let quantity: i32 = row.get(3);
        let filled: i32 = row.get(4);
        let price: f64 = row.get(5);
        let user_id: i32 = row.get(6);
        // let status: &str = row.get(7);

        let order = Order::direct(action, symbol, quantity, filled, price, order_id, user_id);
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
/* Populate the statistics the exchange
 *      - Future note: If we distribute markets across
 *        machines, it might be a good idea to provide
 *        a list of markets to read from.
 **/
pub fn populate_exchange_statistics(exchange: &mut Exchange, conn: &mut Client) {
    for row in conn.query("SELECT total_orders FROM ExchangeStats", &[])
        .expect("Something went wrong in the query.") {

        let total_orders: i32 = row.get(0);
        exchange.total_orders = total_orders;
    }
}

/* TODO: Accept time periods!
 * Read past trades for the requested security from the database.
 * Returns Some(Vec<Trade>) if there are trades,
 * otherwise, returns None.
 **/
pub fn read_trades(symbol: &String, conn: &mut Client) -> Option<Vec<Trade>> {
    let mut trades: Vec<Trade> = Vec::new();
    for row in conn.query("SELECT * FROM ExecutedTrades WHERE symbol=$1", &[&symbol.as_str()]).expect("Read Trades query (History) failed!") {
        let symbol:     &str = row.get(0);
        let action:     &str = row.get(1);
        let price:      f64  = row.get(2);
        let filled_oid: i32  = row.get(3);
        let filled_uid: i32  = row.get(4);
        let filler_oid: i32  = row.get(5);
        let filler_uid: i32  = row.get(6);
        let exchanged:  i32  = row.get(7);
        // let exec_time: &str = row.get(0);
        trades.push(Trade::direct(symbol,
                                  action,
                                  price,
                                  filled_oid,
                                  filled_uid,
                                  filler_oid,
                                  filler_uid,
                                  exchanged
                                  )
                    );
    }
    return Some(trades);
}

/* TODO: Untested, not sure even how to test this.
 * Returns Some(action) if the user owns this pending order, else None. */
pub fn match_pending_order(user_id: i32, order_id: i32, conn: &mut Client) -> Option<String> {
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

/* Writes to the database.
 * This function inserts an order to the Orders table,
 * and will insert it to the PendingOrders table if the
 * order is not COMPLETE.
 **/
pub fn write_insert_order(order: &Order, conn: &mut Client) {
    let query_string = "\
INSERT INTO Orders
(order_ID, symbol, action, quantity, filled, price, user_ID, status)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8);";

    let status: String;
    let mut add_to_pending = false;

    if order.quantity == order.filled {
        status = String::from("COMPLETE");
    } else {
        status = String::from("PENDING");
        add_to_pending = true;
    }
    if let Err(e) = conn.query(query_string, &[ &order.order_id,
                                                &order.symbol,
                                                &order.action,
                                                &order.quantity,
                                                &order.filled,
                                                &order.price,
                                                &order.user_id,
                                                &status,
                                                // None, TODO time_placed
                                                // None  TODO time_updated
    ]) {
        eprintln!("{:?}", e);
        panic!("Something went wrong with the Order Insert query!");
    };
    if add_to_pending {
        let query_string = "INSERT INTO PendingOrders VALUES ($1);";
        if let Err(e) = conn.query(query_string, &[&order.order_id]) {
            eprintln!("{:?}", e);
            panic!("Something went wrong with the PendingOrder Insert query!");
        };
    }

    // Update the exchange total orders
    let query_string = "UPDATE ExchangeStats set total_orders=$1;";
    if let Err(e) = conn.query(query_string, &[&order.order_id]) {
        eprintln!("{:?}", e);
        panic!("Something went wrong with the exchange total orders update query!");
    }

    let query_string: String;
    match &order.action[..] {
        "BUY" => query_string = format!["UPDATE Markets set total_{}=total_{} + 1 where symbol=$1;", "buys", "buys"],
        "SELL" => query_string = format!["UPDATE Markets set total_{}=total_{} + 1 where symbol=$1;", "sells", "sells"],
        _ => panic!("We should never get here.")
    }

    if let Err(e) = conn.query(query_string.as_str(), &[&order.symbol]) {
        eprintln!("{:?}", e);
        panic!("Something went wrong with the Market total count update query!");
    }
}

/* Writes to the database.
 * This function updates a market's statistics.
 **/
pub fn write_update_market_stats(stats: &SecStat, conn: &mut Client) {
    let query_string = "\
UPDATE Markets
SET (total_buys, total_sells, filled_buys, filled_sells, latest_price) =
($1, $2, $3, $4, $5)
WHERE Markets.symbol = $6;";

    if let Err(e) = conn.query(query_string, &[ &stats.total_buys,
                                                &stats.total_sells,
                                                &stats.filled_buys,
                                                &stats.filled_sells,
                                                &stats.last_price.unwrap(),
                                                &stats.symbol
    ]) {
        eprintln!("{:?}", e);
        panic!("Something went wrong with the Market Stats Update query!");
    };
}

/* Writes to the database.
 * This function inserts the trades in the vector into the database.
 */
pub fn write_insert_trades(trades: &Vec<Trade>, conn: &mut Client) {

    let mut query_string = String::new();
    for trade in trades.iter() {
        query_string.push_str(format!["\
INSERT INTO ExecutedTrades
(symbol, action, price, filled_OID, filled_UID, filler_OID, filler_UID, exchanged)
VALUES ('{}', '{}', {}, {}, {}, {}, {}, {}); ", trade.symbol,
                                                trade.action,
                                                trade.price,
                                                trade.filled_oid,
                                                trade.filled_uid,
                                                trade.filler_oid,
                                                trade.filler_uid,
                                                trade.exchanged,
                                    ].as_str());
    }
    if let Err(e) = conn.query(query_string.as_str(), &[]) {
        eprintln!("{:?}", e);
        panic!("Insert Trades query failed!");
    }

}

/* This function takes a string reference, which consists of SQL
 * statements that update the filled counts for relevant rows.
 * */
pub fn update_filled_counts(query_string: &String, conn: &mut Client) {
    if let Err(e) = conn.query(query_string.as_str(), &[]) {
        eprintln!("{:?}", e);
        panic!("Filled Counts Update query failed!");
    }
}

/* Deletes order's from PendingOrders table.
 * Will set order status to COMPLETE and set filled to quantity.
 * */
pub fn delete_pending_orders(order_ids: &Vec<i32>, conn: &mut Client, set_status: &str) {
    // TODO: We can run this all in parallel!
    let mut delete_query_string = String::new();
    let mut update_query_string = String::new();

    for order in order_ids.iter() {
        delete_query_string.push_str(format!["DELETE FROM PendingOrders WHERE order_id={}; ", order].as_str());
        // Determine if order completed or cancelled
        let filled: &str;
        if let "COMPLETE" = set_status {
            filled = "quantity";
        } else {
            filled = "filled";
        }
        update_query_string.push_str(format!["UPDATE Orders SET status='{}', filled={} WHERE order_id={}; ", set_status, filled, order].as_str());
    }
    if let Err(e) = conn.query(delete_query_string.as_str(), &[]) {
        eprintln!("{:?}", e);
        eprintln!("\n{}", delete_query_string);
        panic!("PendingOrders Delete query failed!", );
    }
    if let Err(e) = conn.query(update_query_string.as_str(), &[]) {
        eprintln!("{:?}", e);
        eprintln!("\n{}", update_query_string);
        panic!("Order Status Update query failed!", );
    }
}
