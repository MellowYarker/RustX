use postgres::Client;
use std::collections::BinaryHeap;
use std::cmp::Reverse;

use crate::exchange::{Exchange, Market, Order, SecStat};

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
    for row in conn.query("SELECT total_orders FROM Exchange_Stats", &[])
        .expect("Something went wrong in the query.") {

        let total_orders: i32 = row.get(0);
        exchange.total_orders = total_orders;
    }
}

/* Writes to the database.
 * This function inserts an order to the Orders table.
 **/
pub fn write_insert_order(order: &Order, conn: &mut Client) {
    let query_string = "\
INSERT INTO Orders
(order_ID, symbol, action, quantity, filled, price, user_ID, status)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8);";

    let mut status: String;
    if order.quantity == order.filled {
        status = String::from("COMPLETE");
    } else {
        status = String::from("PENDING");
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
