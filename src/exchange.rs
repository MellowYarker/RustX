use std::collections::{BinaryHeap, HashMap};
use std::cmp::Reverse;

pub mod requests;
pub use crate::exchange::requests::{Order, InfoRequest, Request, Simulation};

pub mod filled;
pub use crate::exchange::filled::FilledOrder;

pub mod stats;
pub use crate::exchange::stats::SecStat;

pub mod market;
pub use crate::exchange::market::Market;

// Error types for price information.
pub enum PriceError {
    NoMarket,
    NoTrades
}

// Represents our exchange's state.
#[derive(Debug)]
pub struct Exchange {
    pub live_orders: HashMap<String, Market>,               // Orders on the market
    pub filled_orders: HashMap<String, Vec<FilledOrder>>,   // Orders that have been filled
    pub statistics: HashMap<String, SecStat>,               // The general statistics of each symbol
    pub total_orders: i32
}

impl Exchange {
    // Create a new exchange on startup
    pub fn new() -> Self {
        let live: HashMap<String, Market> = HashMap::new();
        let filled: HashMap<String, Vec<FilledOrder>> = HashMap::new();
        let stats: HashMap<String, SecStat> = HashMap::new();
        Exchange {
            live_orders: live,
            filled_orders: filled,
            statistics: stats,
            total_orders: 0
        }
    }

    // Initializes the stats for a market given the first order.
    fn init_stats(&mut self, order: Order) {
        let stat = SecStat::from(order);
        self.statistics.insert(stat.symbol.clone(), stat);
        self.total_orders += 1;
    }

    // Update the stats for a market given the new order.
    // We modify total buys/sells, total order, as well as potentially price and filled orders.
    fn update_state(&mut self, order: &Order, executed_trades: Option<Vec<FilledOrder>>) {
        let stats: &mut SecStat = self.statistics.get_mut(&order.security).unwrap();

        // Update the counters and the price
        match &order.action[..] {
            "buy" => {
                stats.total_buys += 1;
            },
            "sell" => {
                stats.total_sells += 1;
            },
            _ => ()
        }

        // Update the price and filled orders if a trade occurred.
        // if let Some(price) = new_price {
        if let Some(mut filled_orders) = executed_trades {
            let price = filled_orders[filled_orders.len() - 1].price;
            stats.update_price(price);
            stats.update_filled_orders(&filled_orders);
            self.extend_past_orders(&mut filled_orders);
        };
        self.total_orders += 1;
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

    /*
    // When we get a new order, we will try to fill it with
    // existing orders on the market. If the order is successfully filled,
    // at least in part, we will update the order's `filled` field, as well
    // as the existing orders it fills.
    //
    // On success, we return a vector of all orders we filled (at least in part),
    // which should then be added to the past orders vector for this market by the
    // caller function.
    //
    // On failure, we return None.
    fn fill_existing_orders(&mut self, order: &mut Order) -> Option<Vec<FilledOrder>> {
        let market = self.live_orders.get_mut(&order.security).expect("Symbol does not exist.");

        // We will populate this if any orders get filled.
        let mut filled_orders: Vec<FilledOrder> = Vec::new();

        let mut new_price = None;
        match &order.action[..] {
            // New buy order, try to fill some existing sells
            "buy" => {
                new_price = market.fill_buy_order(order, &mut filled_orders);
            },
            // New sell order, try to fill some existing buys
            "sell" => {
                new_price = market.fill_sell_order(order, &mut filled_orders);
            },
            _ => () // Not possible
        }

        // Update the market stats as the state has changed.
        match new_price {
            // Price change means orders were filled
            Some(_) => {
                return Some(filled_orders);
            },
            None => return None
        }
    }
    */

    // Extends the past orders vector
    fn extend_past_orders(&mut self, new_orders: &mut Vec<FilledOrder>) {

        // Default initialize the past orders market if it doesn't already exist
        let default_type: Vec<FilledOrder> = Vec::new();
        let market = self.filled_orders.entry(new_orders[0].security.clone()).or_insert(default_type);
        market.append(new_orders);
    }

    // Print a market
    pub fn show_market(&self, symbol: &String) {
        let market = self.live_orders.get(symbol).expect("NO VALUE");
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

    // Shows the history of orders in this market.
    pub fn show_market_history(&self, symbol: &String) {
        let market = self.filled_orders.get(symbol).expect("The symbol that was requested either doesn't exist or has no past trades.");

        println!("\nMarket History: ${}", symbol);
        println!("\t\t| Filled by Order | Order | Shares Exchanged | Price |");
        println!("\t\t------------------------------------------------------");
        for past_order in market {
            println!("\t\t|\t{}\t\t{}\t     {}\t  \t${:.2}   |", past_order.filled_by, past_order.id, past_order.exchanged, past_order.price);
        }
        println!("\t\t------------------------------------------------------\n");
    }
    /* Add an order to the security's order list.
     If the security isn't in the HashMap, create it.
     Returns a reference to the order on success.

     TODO: Failure condition? Different return type?
    */
    pub fn add_order_to_security(&mut self, order: Order) {

        let action = &order.action.clone()[..];

        let mut order: Order = order;
        // Set the order_id for the order.
        order.order_id = self.total_orders + 1;

        // Try to access the security in the HashMap
        match self.live_orders.get_mut(&order.security) {
        // if self.live_orders.contains_key(&order.security) {

            Some(market) => {
                // Try to fill the new order with existing orders on the market.
                // let filled_orders = self.fill_existing_orders(&mut order);
                let filled_orders = market.fill_existing_orders(&mut order);

                // Add the new order to the buy/sell vec if it wasn't completely filled
                if order.quantity != order.filled {

                    let entry = self.live_orders.get_mut(&order.security).unwrap();
                    match action {
                        "buy" => {
                            entry.buy_orders.push(order.clone());
                        },
                        "sell" => {
                            // Sell is a min heap so we reverse the comparison
                            entry.sell_orders.push(Reverse(order.clone()));
                        },
                        _ => ()
                    }
                } else {
                    // TEST SPEED
                    // println!("The order has been filled!");
                }
                self.update_state(&order, filled_orders);
            },
            None => {
        // } else {
                // Entry doesn't exist, create it.
                // buy is a max heap, sell is a min heap.
                let mut buy_heap: BinaryHeap<Order> = BinaryHeap::new();
                let mut sell_heap: BinaryHeap<Reverse<Order>> = BinaryHeap::new();
                match action {
                    "buy" => {
                        buy_heap.push(order.clone());
                    },
                    "sell" => {
                        sell_heap.push(Reverse(order.clone()));
                    },
                    // We can never get here.
                    _ => ()
                };

                let new_market = Market::new(buy_heap, sell_heap);
                self.live_orders.insert(order.security.clone(), new_market);

                // Since this is the first order, initialize the stats for this security.
                self.init_stats(order);
            }
        }
    }

    /* Allows a user to simulate a market.
     *
     * Pre-conditions:
     *  - The market must exist.
     *  - Market must have a set price, i.e a trade must have occured.
     *  - There must be at least 1 order on the market.
     *
     * TODO
     * If these preconditions are not met, we will return an error.
     * Otherwise, we return the number of trades that took place.
     * */
    pub fn simulate_market(&mut self, sim: &Simulation) -> Result<i32, ()> {

        match self.get_price(&sim.symbol) {
            Ok(_) => (),
            Err(_) => {
                return Err(()); // TODO better error handling
            }
        };

        // Simulation loop
        for _time_step in 0..sim.duration {
            // We want to randomly decide to buy or sell,
            // then perform a random walk from the current price, exchanging within
            // say 1 standard deviation of the mean # of shares per trade.
            let rand: f64 = random!(); // quick 0.0 ~ 1.0 generation
            let mut action: String = String::new();

            if rand < 0.5 {
                action.push_str("buy");
            } else {
                action.push_str("sell");
            }

            let mut current_price = 0.0;
            if let Ok(p) = self.get_price(&sim.symbol) {
                current_price = p;
            };

            // Deviate from the current price
            let price_deviation: i8 = random!(-5..=5); // Deviation of +/- 5%
            let new_price = current_price + current_price * (price_deviation as f64 / 100.0);

            // Chose the number of shares
            let shares:i32 = random!(2..=13); // TODO: get random number of shares

            // Create the order and send it to the market
            let order = Order::from(action, sim.symbol.clone(), shares, new_price);
            self.add_order_to_security(order)
        }

        return Ok(0);
    }
}
