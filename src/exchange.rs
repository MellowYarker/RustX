use std::collections::{HashMap};
use std::cmp::Ordering;

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
        self.total_orders = 1;
    }

    // Update the stats for a market given the new order.
    fn update_stats(&mut self, order: Order) {
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

    /* Given a buy order, try to fill it with existing sell orders in the market.
     * Add any orders that are completely filled into the filled_orders vector.
     *
     * Returns the lowest sell price that was filled or None if no trade occured.
     */
    fn fill_buy_order(highest_bid: &mut Order, market: &mut Market, filled_orders: &mut Vec<FilledOrder>) -> Option<f64> {

        // No trades by default
        let mut new_price = None;

        // Loop until no more orders can be filled.
        loop {
            // No more sell orders to fill, or the buy order was filled.
            if market.sell_orders.len() == 0 || (highest_bid.quantity == highest_bid.filled) {
                break;
            }

            // We try to fill the lowest sell
            // Unlike in fill_sell_order, we do not remove this element since it would
            // necessitate a move of all other elements one index to the left (expensive).
            let lowest_offer = &market.sell_orders[0];

            let lowest_sell_remaining = lowest_offer.quantity - lowest_offer.filled;
            let highest_bid_remaining = highest_bid.quantity - highest_bid.filled;

            if lowest_offer.price <= highest_bid.price {

                // Update the price
                new_price = Some(lowest_offer.price);

                // If more shares are being bought than sold
                if lowest_sell_remaining <= highest_bid_remaining {
                    let amount_traded = lowest_sell_remaining;

                    // Update the orders
                    let mut update_lowest = lowest_offer.clone();
                    update_lowest.filled += amount_traded;

                    highest_bid.filled += amount_traded;

                    // Remove order from sell vector as it has been filled.
                    market.sell_orders.remove(0);

                    // Since the sell has been filled, add it to the new vector.
                    filled_orders.push(FilledOrder::order_to_filled_order(&update_lowest, &highest_bid, amount_traded));

                    // If the newly placed order was consumed
                    /*
                    if lowest_sell_remaining == highest_bid_remaining {
                        // TODO: Do we really want to do this in this way?
                        // filled_orders.push(highest_bid.clone());
                        filled_orders.push(FilledOrder::order_to_filled_order(&highest_bid, &update_lowest, amount_traded));
                    }
                    */
                } else {
                    // The buy order was completely filled.
                    let amount_traded = highest_bid_remaining;

                    let mut update_lowest = lowest_offer.clone();
                    update_lowest.filled += amount_traded;

                    highest_bid.filled  += amount_traded;

                    // Newly placed order was filled
                    // TODO: Do we really want to do this in this way?
                    filled_orders.push(FilledOrder::order_to_filled_order(&update_lowest, &highest_bid, amount_traded));

                    // Update the lowest offer.
                    market.sell_orders.remove(0);
                    market.sell_orders.insert(0, update_lowest);

                }
            } else {
                // Highest buy doesn't reach lowest sell.
                break;
            }
        }

        return new_price;
    }

    /* Given a sell order, try to fill it with existing buy orders in the market.
     * Add any orders that are completely filled into the filled_orders vector.
     *
     * Returns the highest buy price that was filled or None if no trade occured.
    */
    fn fill_sell_order(lowest_offer: &mut Order, market: &mut Market, filled_orders: &mut Vec<FilledOrder>) -> Option<f64> {
        // No trades by default
        let mut new_price = None;

        // Loop until no more orders can be filled.
        loop {
            // The new sell order was filled.
            if lowest_offer.quantity == lowest_offer.filled {
                break;
            }

            // We try to fill the highest buy
            let highest_bid = match market.buy_orders.pop() { // May potentially add back to vector if not filled.
                Some(bid) => bid,
                None => return new_price // No more buy orders to fill
            };

            let lowest_sell_remaining = lowest_offer.quantity - lowest_offer.filled;
            let highest_bid_remaining = highest_bid.quantity - highest_bid.filled;

            if lowest_offer.price <= highest_bid.price {

                // Update the price
                new_price = Some(highest_bid.price);

                // If more shares are being sold than bought
                if highest_bid_remaining <= lowest_sell_remaining {
                    let amount_traded = highest_bid_remaining;

                    // Update the orders
                    let mut update_highest = highest_bid.clone();
                    update_highest.filled += amount_traded;

                    lowest_offer.filled += amount_traded;

                    // Add the updated buy to the Vector we return
                    filled_orders.push(FilledOrder::order_to_filled_order(&update_highest, &lowest_offer, amount_traded));

                    /*
                    // If the newly placed order was consumed
                    if lowest_sell_remaining == highest_bid_remaining {
                        // TODO: Do we really want to do this in this way?
                        filled_orders.push(FilledOrder::order_to_filled_order(&lowest_offer, &update_highest, amount_traded));
                    }
                    */
                } else {
                    // The sell order was completely filled.
                    let amount_traded = lowest_sell_remaining;

                    let mut update_highest = highest_bid.clone();
                    update_highest.filled += amount_traded;

                    lowest_offer.filled += amount_traded;

                    // Newly placed order was filled
                    // TODO: Do we really want to do this in this way?
                    filled_orders.push(FilledOrder::order_to_filled_order(&update_highest, &lowest_offer, amount_traded));

                    // Update the highest bid.
                    market.buy_orders.push(update_highest);

                }
            } else {
                // Lowest sell doesn't reach highest buy.
                market.buy_orders.push(highest_bid); // Put the highest bid back.
                break;
            }
        }

        return new_price
    }

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
                new_price = Exchange::fill_buy_order(order, market, &mut filled_orders);
            },
            // New sell order, try to fill some existing buys
            "sell" => {
                new_price = Exchange::fill_sell_order(order, market, &mut filled_orders);
            },
            _ => () // Not possible
        }

        // Update the market's price if it changed
        match new_price {
            // Price change means orders were filled
            Some(price) => {
                let stats = self.statistics.get_mut(&order.security).expect("ERROR: Symbol doesn't exist.");
                stats.update_price(price);
                stats.update_filled_orders(&filled_orders);
                return Some(filled_orders);
            },
            None => return None
        }
    }

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
        println!("\nMarket: ${}", symbol);
        println!("\t--BUYS--");
        println!("\t\t| ID | Price \t| Quantity | Filled |");
        println!("\t\t-------------------------------------");
        for order in &market.buy_orders {
            println!("\t\t| {}\t${}\t     {}\t  \t{}   |", order.order_id, order.price, order.quantity, order.filled);
        }
        println!("\t\t-------------------------------------\n");

        println!("\t--SELLS--");
        println!("\t\t| ID | Price \t| Quantity | Filled |");
        println!("\t\t-------------------------------------");
        for order in &market.sell_orders {
            println!("\t\t| {}\t${}\t     {}\t  \t{}   |", order.order_id, order.price, order.quantity, order.filled);
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
            println!("\t\t|\t{}\t\t{}\t     {}\t  \t${}   |", past_order.filled_by, past_order.id, past_order.exchanged, past_order.price);
        }
        println!("\t\t------------------------------------------------------\n");
    }
    /* Add an order to the security's order list.
     If the security isn't in the HashMap, create it.
     Returns a reference to the order on success.

     TODO: Failure condition? Different return type?
    */
    pub fn add_order_to_security(&mut self, order: Order) {

        let action = order.action.clone();

        let mut order: Order = order;
        // Set the order_id for the order.
        order.order_id = self.total_orders + 1;

        // Try to access the security in the HashMap
        if self.live_orders.contains_key(&order.security) {

            // Update the market and then the statistics.
            match self.fill_existing_orders(&mut order) {
                Some(mut orders) => {
                    println!("The order just placed filled the following orders:");

                    for ord in &orders {
                        println!("Order ({}) filled order ({}) at ${}. Exchanged {} shares.", ord.filled_by, ord.id, ord.price, ord.exchanged);
                    }

                    // Move the recently filled orders into the filled_orders array
                    self.extend_past_orders(&mut orders);
                },
                None => {
                    println!("The order has been added to the market.");
                }
            }

            // Add the new order to the buy/sell vec if it wasn't completely filled
            if order.quantity != order.filled {

                let entry = self.live_orders.get_mut(&order.security).unwrap();

                let market_order_vec = match &action[..] {
                    "buy" => &mut entry.buy_orders,
                    _ => &mut entry.sell_orders, // only other possibility is sell
                };

                // We order by price primarily
                match market_order_vec.binary_search(&order) {
                    Ok(pos) => {
                        // TODO: We should insert this in a way such that it is filled after the
                        // other orders with the same price that were placed first!
                        market_order_vec.insert(pos, order.clone());
                    },
                    Err(pos) => {
                        market_order_vec.insert(pos, order.clone());
                    }
                }
            } else {
                println!("The order has been filled!");
            }

            // Update the stats because a new order has been placed.
            self.update_stats(order.clone());
        } else {
            // Entry doesn't exist, create it.
            let mut buy_vec : Vec<Order> = Vec::new();
            let mut sell_vec: Vec<Order> = Vec::new();
            match &action[..] {
                "buy" => {
                    buy_vec.push(order.clone());
                },
                "sell" => {
                    sell_vec.push(order.clone());
                },
                // We can never get here.
                _ => ()
            };

            let new_market = Market::new(buy_vec, sell_vec);
            self.live_orders.insert(order.security.clone(), new_market);

            // Since this is the first order, initialize the stats for this security.
            self.init_stats(order);
        }
    }
}

// The market for a security
// TODO: Do we want orders stored in a sorted binary tree instead?
#[derive(Debug)]
pub struct Market {
    pub buy_orders: Vec<Order>,
    pub sell_orders: Vec<Order>
}

impl Market {
    pub fn new(buy: Vec<Order>, sell: Vec<Order>) -> Self {
        Market {
            buy_orders: buy,
            sell_orders: sell
        }
    }
}

// Statistics about a security
#[derive(Debug)]
pub struct SecStat {
    pub symbol: String,
    pub total_buys: i32,
    pub total_sells: i32,
    pub filled_buys: i32,
    pub filled_sells: i32,
    pub last_price: Option<f64>, // Last price we got
}

impl SecStat {
    pub fn from(order: Order) -> Self {

        let symbol = order.security.clone();

        let total_buys = match &order.action[..] {
            "buy" => 1,
            _ => 0
        };

        let total_sells  = match &order.action[..] {
            "sell" => 1,
            _ => 0
        };

        // let last_price = order.price;
        let last_price = None;

        SecStat {
            symbol: symbol,
            total_buys: total_buys,
            total_sells: total_sells,
            filled_buys: 0,
            filled_sells: 0,
            last_price: last_price
        }
    }

    // Updates the price, returns the difference.
    fn update_price(&mut self, new_price: f64) -> f64 {
        match self.last_price {
            Some(price) => {
                let diff = price - new_price;
                self.last_price = Some(new_price);
                return diff;
            },
            None => {
                self.last_price = Some(new_price);
                return new_price;
            }
        }
    }

    // Iterates over the vector of filled orders and
    // updates the filled buy or sell count.
    fn update_filled_orders(&mut self, filled_orders: &Vec<FilledOrder>) {
        for order in filled_orders {
            match &order.action[..] {
                "buy" => {
                    self.filled_buys += 1;
                },
                "sell" => {
                    self.filled_sells += 1;
                },
                _ => ()
            }
        }
    }
}

#[derive(Debug)]
pub struct FilledOrder {
    pub action: String,
    pub security: String,
    pub price: f64,         // price at which this order was filled
    pub id: i32,            // this order's ID
    pub filled_by: i32,     // the order ID that filled this order
    pub exchanged: i32      // the amount of shares exchanged
}

impl FilledOrder {
    fn from(action: &String, security: &String, price: f64, id: i32, filled_by: i32, exchanged: i32) -> Self {
        FilledOrder {
            action: action.clone(),
            security: security.clone(),
            price,
            id,
            filled_by,
            exchanged
        }
    }

    // Create a FilledOrder from a pair of orders.
    pub fn order_to_filled_order(old: &Order, filler: &Order, exchanged: i32) -> Self {
        FilledOrder::from(&old.action, &old.security, old.price, old.order_id, filler.order_id, exchanged)
    }
}

// An order type for a security
#[derive(Debug)]
pub struct Order {
    pub action: String,     // BUY or SELL
    pub security: String,   // Symbol
    pub quantity: i32,
    pub filled: i32,        // Quantity filled so far
    pub price: f64,
    pub order_id: i32
}

impl Order {
    pub fn from(action: String, security: String, quantity: i32, price: f64) -> Order {
        Order {
            action,
            security,
            quantity,
            filled: 0,
            price,
            order_id: 0 // Updated later.
        }
    }
}

impl Clone for Order {
    fn clone(&self) -> Self {
        Order {
            action: self.action.clone(),
            security: self.security.clone(),
            ..*self
        }
    }
}

impl Ord for Order {
    fn cmp(&self, other: &Self) -> Ordering {
        if let Ordering::Equal = &self.security.cmp(&other.security) {
            if self.price < other.price {
                return Ordering::Less;
            } else if other.price < self.price {
                return Ordering::Greater;
            }
            return Ordering::Equal;
        } else {
            return Ordering::Equal;
        }
    }
}

impl PartialOrd for Order {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        &self.security == &other.security && self.price == other.price
    }
}

impl Eq for Order { }

// Non-orders requests like price of a security,
// TODO: number of buy or sell orders in a market, etc.
pub struct InfoRequest {
    pub action: String,
    pub symbol: String
}

impl InfoRequest {
    pub fn new(action: String, symbol: String) -> Self {
        InfoRequest {
            action,
            symbol
        }
    }
}

// Possible requests from a user
pub enum Request {
    OrderReq(Order),
    InfoReq(InfoRequest),
}
