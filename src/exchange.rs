use std::collections::{BinaryHeap, HashMap};
use std::cmp::Reverse;

pub mod requests;
pub use crate::exchange::requests::{Order, InfoRequest, CancelOrder, Request, Simulation, OrderStatus};

pub mod filled;
pub use crate::exchange::filled::Trade;

pub mod stats;
pub use crate::exchange::stats::SecStat;

pub mod market;
pub use crate::exchange::market::Market;

pub use crate::account::{UserAccount, Users};

pub use crate::database;

pub use crate::buffer::BufferCollection;

use postgres::{Client, NoTls};

// Error types for price information.
pub enum PriceError {
    NoMarket,
    NoTrades
}

// Represents our exchange's state.
#[derive(Debug)]
pub struct Exchange {
    pub live_orders: HashMap<String, Market>,    // Orders on the market
    pub has_trades: HashMap<String, bool>,
    pub statistics: HashMap<String, SecStat>,    // The general statistics of each symbol
    pub total_orders: i32
}

impl Exchange {
    // Create a new exchange on startup
    pub fn new() -> Self {
        let live_orders: HashMap<String, Market> = HashMap::new();
        let has_trades: HashMap<String, bool> = HashMap::new();
        let statistics: HashMap<String, SecStat> = HashMap::new();
        Exchange {
            live_orders,
            has_trades,
            statistics,
            total_orders: 0
        }
    }

    /* Update the stats for a market given the new order.
     * We modify total buys/sells, total order, as well as potentially price and filled orders.
     *
     * Returns Some(price) if trade occured, or None.
     */
    fn update_state(&mut self, order: &Order, users: &mut Users, buffers: &mut BufferCollection, executed_trades: Option<Vec<Trade>>, conn: &mut Client) -> Option<f64> {

        let stats: &mut SecStat = self.statistics.get_mut(&order.symbol).unwrap();
        stats.modified = true;

        // Write the newly placed order to the Orders table.
        // If Order isn't complete, adds to pending as well.
        database::write_insert_order(order, conn);

        // Update the counters and the price
        match &order.action[..] {
            "BUY" => {
                stats.total_buys += 1;
            },
            "SELL" => {
                stats.total_sells += 1;
            },
            _ => ()
        }

        let mut new_price = None;

        // Update the price and filled orders if a trade occurred.
        if let Some(mut trades) = executed_trades {
            let price = trades[trades.len() - 1].price;
            new_price = Some(price);
            // Updates in-mem data
            stats.update_market_stats(price, &trades);
            // Updates database
            database::write_update_market_stats(stats, conn);

            /* TODO: Updating accounts seems like something that
             *       shouldn't slow down order execution.
             *
             * Market state doesn't depend on users view of the market.
             * This function is also computationally expensive, I think
             * the better route is to compute this in a separate thread,
             * and somehow force sequential access of users accounts
             * (think mutex locks, and maybe write filled orders to a buffer
             * in the mean time?)
             */
            // Updates database too.
            users.update_account_orders(&mut trades, buffers, conn);
            self.has_trades.insert(order.symbol.clone(), true);
        };

        self.total_orders += 1;
        return new_price;
    }

    /* Returns the price of the given symbol, or one of two errors.
     * Err:
     *  - No market found: No orders have been placed
     *  - No trades executed: Orders may have been placed, but no trade = no price.
     */
    pub fn get_price(&self, symbol: &String) -> Result<f64, PriceError> {
        // Get the market
        let stats = match self.statistics.get(symbol) {
            Some(stat) => stat,
            None => {
                return Err(PriceError::NoMarket);
            }
        };

        // Get the price
        match stats.last_price {
            Some(_) => Ok(stats.last_price.unwrap()),   // safe to unwrap this!
            None => {
                Err(PriceError::NoTrades)
            }
        }
    }

    // Print a market
    pub fn show_market(&self, symbol: &String) {
        let market = match self.live_orders.get(symbol) {
            Some(market) => market,
            None => {
                println!("${} has no pending orders!", symbol);
                return;
            }
        };
        let num_orders_to_view = 10;

        println!("\nMarket: ${}", symbol);

        println!("\t--SELLS--");
        println!("\t\t| ID | Price \t| Quantity | Filled |");
        println!("\t\t-------------------------------------");

        let sells = market.sell_orders.clone().into_sorted_vec();
        let start = std::cmp::min(sells.len(), num_orders_to_view);
        let lowest_sells = &sells[sells.len() - start ..];

        for result in lowest_sells.iter() {
            let order = &result.0;
            println!("\t\t| {}\t${:.2}\t     {}\t  \t{}   |", order.order_id, order.price, order.quantity, order.filled);
        }
        println!("\t\t-------------------------------------\n");

        println!("\t--BUYS--");
        println!("\t\t| ID | Price \t| Quantity | Filled |");
        println!("\t\t-------------------------------------");
        let buys = market.buy_orders.clone().into_sorted_vec();
        let mut order_count = 0;
        for order in buys.iter().rev() {
            order_count += 1;
            println!("\t\t| {}\t${:.2}\t     {}\t  \t{}   |", order.order_id, order.price, order.quantity, order.filled);
            if order_count == num_orders_to_view {
                break
            }
        }
        println!("\t\t-------------------------------------\n");


        let market = self.statistics.get(symbol).expect("NO VALUE");
        println!("STATS");
        println!("\t{:?}", market);

    }

    // TODO: Once we store time, lets include timeframes?
    //       Might be good for graphing price.
    // Shows the history of orders in this market.
    pub fn show_market_history(&self, symbol: &String, conn: &mut Client) {
        if let Some(trades) = database::read_trades(symbol, conn) {
            println!("\nMarket History: ${}", symbol);
            println!("\t\t| Filled by Order | Order | Shares Exchanged | Price |");
            println!("\t\t------------------------------------------------------");
            for past_order in trades {
                println!("\t\t|\t{}\t\t{}\t     {}\t  \t${:.2}   |", past_order.filler_oid, past_order.filled_oid, past_order.exchanged, past_order.price);
            }
            println!("\t\t------------------------------------------------------\n");
        } else {
            eprintln!("The security that was requested either doesn't exist or has no past trades.");
        }
    }

    /* Add an order to the market's order list,
     * and may fill pending orders whose conditions are satisfied.
     * Assumes user has already been authenticated.
     *
     * Returns the new price if trade occurred, otherwise, None or errors.
    */
    pub fn submit_order_to_market(&mut self, users: &mut Users, buffers: &mut BufferCollection, order: Order, username: &String, auth: bool, conn: &mut Client) -> Result<Option<f64>, String> {

        // Mutable reference to the account associated with given username.
        let account = match users.get_mut(username, auth) {
            Ok(acc) => acc,
            Err(e) => {
                Users::print_auth_error(e);
                return Err("".to_string());
            }
        };
        let mut order: Order = order;
        let mut new_price = None; // new price if trade occurs

        // PER-6 account is being modified so set modified to true.
        account.modified = true;

        // Set the order_id for the order.
        order.order_id = self.total_orders + 1;

        // Try to access the security in the HashMap
        match self.live_orders.get_mut(&order.symbol) {
            Some(market) => {
                // Try to fill the new order with existing orders on the market.
                let trades = market.fill_existing_orders(&mut order);

                // Add the new order to the buy/sell heap if it wasn't completely filled,
                // as well as the users account.
                if order.quantity != order.filled {
                    match &order.action[..] {
                        "BUY" => {
                            market.buy_orders.push(order.clone());
                        },
                        "SELL" => {
                            // Sell is a min heap so we reverse the comparison
                            market.sell_orders.push(Reverse(order.clone()));
                        },
                        _ => ()
                    }

                    // Add to this accounts pending orders.
                    let current_market = account.pending_orders.entry(order.symbol.clone()).or_insert(HashMap::new());
                    current_market.insert(order.order_id, order.clone());
                }

                // Add this new order to the database buffer
                buffers.buffered_orders.add_unknown_to_order_buffer(&order);

                // Update the state of the exchange.
                new_price = self.update_state(&order, users, buffers, trades, conn);
            },
            // The market doesn't exist, create it if found in DB,
            // otherwise the user entered a market that DNE.
            None => {
                if database::read_market_exists(&order.symbol, conn) {
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
                    self.live_orders.insert(order.symbol.clone(), new_market);

                    // Add the symbol name and order to this accounts pending orders.
                    let new_account_market = account.pending_orders.entry(order.symbol.clone()).or_insert(HashMap::new());
                    new_account_market.insert(order.order_id, order.clone());

                    // Add this new order to the database buffer
                    buffers.buffered_orders.add_unknown_to_order_buffer(&order);

                    // Since this is the first order, initialize the stats for this security.
                    new_price = self.update_state(&order, users, buffers, None, conn);
                } else {
                    return Err(format!["The market ${} was not found in the database. User error!", order.symbol]);
                }
            }
        }

        return Ok(new_price);
    }

    /* Cancel the order in the given market with the given order ID.
     *
     * The user has been authenticated by this point, however we still
     * need to ensure that the order being cancelled was placed by them.
     *
     * Note: Just like real exchanges, cancelling an order means cancelling
     *       whatever *remains* of an order, i.e any fulfilled portion
     *       cannot be cancelled.
     * */
    pub fn cancel_order(&mut self, order_to_cancel: &CancelOrder, users: &mut Users, buffers: &mut BufferCollection, conn: &mut Client) -> Result<(), String>{
        if let Ok(account) = users.get(&(order_to_cancel.username), true) {
            // 1. Ensure the order belongs to the user
            if let Some(action) = account.user_placed_pending_order(&order_to_cancel.symbol, order_to_cancel.order_id, conn) {
                if let Some(market) = self.live_orders.get_mut(&(order_to_cancel.symbol)) {
                    // 2. Remove order from the market
                    match &action[..] {
                        "BUY" => {
                            // Move all the orders except the one we're cancelling to a new heap,
                            // then move it back to the buy heap.
                            let new_size = market.buy_orders.len() - 1;
                            let mut temp = BinaryHeap::with_capacity(new_size);
                            for order in market.buy_orders.drain().filter(|order| order.order_id != order_to_cancel.order_id) {
                                temp.push(order); // Worst case is < O(n) since we preallocate
                            }
                            market.buy_orders.append(&mut temp);
                        },
                        "SELL" => {
                            // Move all the orders except the one we're cancelling to a new heap,
                            // then move it back to the sell heap.
                            let new_size = market.sell_orders.len() - 1;
                            let mut temp = BinaryHeap::with_capacity(new_size);
                            for order in market.sell_orders.drain().filter(|order| order.0.order_id != order_to_cancel.order_id) {
                                temp.push(order); // Worst case is < O(n) since we preallocate
                            }
                            market.sell_orders.append(&mut temp);
                        },
                        _ => () // no other possibilities
                    }

                    // 3. Remove order from users account
                    if let Ok(account) = users.get_mut(&(order_to_cancel.username), true) {
                        account.remove_order_from_account(&(order_to_cancel.symbol), order_to_cancel.order_id);

                        // Indicate that the user's account has been modified.
                        account.modified = true;
                    }

                    // TODO: Do we want to update market stats? total_cancelled maybe?
                    //       If we do, we have to also set stats.modified = true
                    let mut to_remove = Vec::new();
                    to_remove.push(order_to_cancel.order_id);

                    // Add this cancellation to the database buffer.
                    let order = Order::from_cancelled(order_to_cancel.order_id);
                    buffers.buffered_orders.add_or_update_entry_in_order_buffer(&order, false); // PER-5 update

                    // TODO: PER-6/7
                    //       Remove this db write eventually, we just write the buffers.
                    database::write_delete_pending_orders(&to_remove, conn, OrderStatus::CANCELLED);

                    return Ok(());

                } else {
                    panic!("The market that we want to cancel an order from doesn't exist.\
                            This shouldn't ever happen since we've already verified that the user has placed an order in this market!"
                    );
                }
            } else {
                return Err("The order requested to be cancelled was not found in the associated user's pending orders!".to_string());
            }
        }
        panic!("Could not find the user while cancelling an order.\
                This shouldn't happen ever, since we've already authenticated the user!"
        );
    }

    /* Simulate trades, currently just for bandwidth testing.
     * TODO:
     *      - Maybe simulate individual markets? (This was old behaviour)
     *          - Could be interesting if we want to try some arbitrage algos later?
     **/
    pub fn simulate_market(&mut self, sim: &Simulation, users: &mut Users, buffers: &mut BufferCollection, conn: &mut Client) {

        let mut test_client = Client::connect("host=localhost user=postgres dbname=test_db", NoTls).expect("Failed to access test db");

        let buy = String::from("BUY");
        let sell = String::from("SELL");

        let mut usernames: Vec<String> = Vec::with_capacity(sim.trader_count as usize);
        let mut i = 0;

        // Fill usernames with user_{num}
        while i != sim.trader_count {
            let name = format!("user_{}", i);
            usernames.push(name);
            i += 1;
        }

        i = 0;
        let mut markets: Vec<String> = Vec::with_capacity(sim.market_count as usize);
        let mut prices: Vec<f64> = Vec::with_capacity(sim.market_count as usize);

        // Fill markets
        database::read_exchange_markets_simulations(&mut markets, conn);
        if markets.len() != (sim.market_count as usize) {
            panic!("{} markets is not {} markets!", markets.len(), sim.market_count);
        }

        while i != sim.market_count {
            prices.push(10.0); // The price doesn't matter for bandwidth testing
            i += 1;
        }

        let mut action: &String;
        let mut username: &String;

        for name in usernames.iter() {
            users.new_account(UserAccount::from(name, &"password".to_string()), conn);
        }

        // Simulation loop
        for _time_step in 0..sim.duration {
            // We want to randomly decide to buy or sell,
            // then perform a random walk from the current price, exchanging within
            // say 1 standard deviation of the mean # of shares per trade.

            let rand: f64 = random!(); // quick 0.0 ~ 1.0 generation
            if rand < 0.5 {
                action = &buy;
            } else {
                action = &sell;
            }
            let user_index = random!(0..=sim.trader_count - 1);
            username = &usernames[user_index as usize];

            let market_index = random!(0..=sim.market_count - 1);
            let symbol = &markets[market_index as usize];

            let current_price = match self.get_price(symbol) {
                Ok(price) => price,
                Err(_) => prices[market_index as usize]
            };

            // Deviate from the current price
            let price_deviation: i8 = random!(-5..=5); // Deviation of +/- 5%
            let new_price = current_price + current_price * (price_deviation as f64 / 100.0);

            // Choose the number of shares
            let shares:i32 = random!(2..=13); // TODO: get random number of shares

            if let Ok(account) =  users.authenticate(username, &"password".to_string(), conn) {
                // Create the order and send it to the market
                let order = Order::from(action.to_string(), symbol.to_string().clone(), shares, new_price, OrderStatus::PENDING, account.id);
                if account.validate_order(&order) {
                    if let Err(e) = self.submit_order_to_market(users, buffers, order, username, true, conn) {
                        eprintln!("{}", e);
                    }
                }
            }
            // buffers.update_buffer_states(&self.statistics, conn);
            buffers.update_buffer_states(&self, &mut test_client);
        }

        // If you want prints of each users account, uncomment this.
        // users.print_all();
    }
}
