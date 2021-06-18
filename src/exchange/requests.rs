use std::cmp::Ordering;
use crate::account::UserAccount;

// The status of an order, each is 1 byte (u8)
#[derive(Copy, Clone, Debug)]
pub enum OrderStatus {
    PENDING,
    COMPLETE,
    CANCELLED
}

// An order type for a security
#[derive(Debug)]
pub struct Order {
    pub action: String,     // buy or sell
    pub symbol: String,     // Symbol of this security
    pub quantity: i32,
    pub filled: i32,        // Quantity filled so far
    pub price: f64,
    pub order_id: i32,
    pub status: OrderStatus,
    pub user_id: Option<i32>// user ID of user who placed order, starts as None during tokenization.
}

impl Order {
    // Used when reading a user from the frontend
    pub fn from(action: String, symbol: String, quantity: i32, price: f64, status: OrderStatus, user_id: Option<i32>) -> Self {
        // Truncate price to 2 decimal places
        let price = f64::trunc(price  * 100.0) / 100.0;

        Order {
            action,
            symbol,
            quantity,
            filled: 0,
            price,
            order_id: 0, // Updated later.
            status,
            user_id
        }
    }

    // Used when reading an existing user from the database
    pub fn direct(action: &str, symbol: &str, quantity: i32, filled: i32, price: f64, order_id: i32, status: OrderStatus, user_id: i32) -> Self {
        // Truncate price to 2 decimal places
        let price = f64::trunc(price  * 100.0) / 100.0;

        // TODO: Need to include order status and time placed/updated.
        Order {
            action: action.to_string().clone(),
            symbol: symbol.to_string().clone(),
            quantity,
            filled,
            price,
            order_id,
            status,
            user_id: Some(user_id)
        }
    }
}

impl Clone for Order {
    fn clone(&self) -> Self {
        Order {
            action: self.action.clone(),
            symbol: self.symbol.clone(),
            ..*self
        }
    }
}

impl Ord for Order {
    fn cmp(&self, other: &Self) -> Ordering {
        if let Ordering::Equal = &self.symbol.cmp(&other.symbol) {
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
        &self.symbol == &other.symbol && self.price == other.price
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

// Allows us to perform simulations on our market
pub struct Simulation {
    pub action: String,
    pub trader_count: u32,
    pub market_count: u32,
    pub duration: u32
}

impl Simulation {
    pub fn from(action: String, trader_count: u32, market_count: u32, duration: u32) -> Self {
        Simulation {
            action,
            trader_count,   // number of traders
            market_count,   // number of markets to trade in
            duration        // number of trades to make
        }
    }
}

pub struct CancelOrder {
    pub symbol: String,
    pub order_id: i32,
    pub username: String,
}

pub enum Request {
    OrderReq(Order, String, String),// first string is username, second password
    CancelReq(CancelOrder, String), // string is password
    InfoReq(InfoRequest),
    SimReq(Simulation),
    UserReq(UserAccount, String), // Account followed by action
    UpgradeDbReq(String, String), // username, password. Only admin can call this
}
