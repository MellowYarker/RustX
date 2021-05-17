use std::cmp::Ordering;
use crate::account::UserAccount;
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
        // Truncate price to 2 decimal places
        let price = f64::trunc(price  * 100.0) / 100.0;

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

// Allows us to perform simulations on our market
pub struct Simulation {
    pub action: String,
    pub symbol: String,
    pub duration: u32
}

impl Simulation {
    pub fn from(action: String, symbol: String, duration: u32) -> Self {
        Simulation {
            action,
            symbol,
            duration
        }
    }
}

pub enum Request {
    OrderReq(Order, String, String), // first string is username, second password
    InfoReq(InfoRequest),
    SimReq(Simulation),
    UserReq(UserAccount)
}
