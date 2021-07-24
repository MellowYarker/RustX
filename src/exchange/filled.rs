use crate::exchange::Order;
use chrono::{DateTime, Utc};

/* Note that a trade does not indicate a full order was processed!
 * It may have only filled part of an order.
 **/
#[derive(Debug)]
pub struct Trade {
    pub action: String,
    pub symbol: String,
    pub price: f64,         // price at which this trade was occured
    pub filled_oid: i32,    // ID of order getting filled
    pub filled_uid: i32,    // ID of user who placed the order that is being filled
    pub filler_oid: i32,    // ID of new order that triggered the trade
    pub filler_uid: i32,    // ID of user who placed new order that triggered the trade
    pub exchanged: i32,     // the amount of shares exchanged
    pub execution_time: DateTime<Utc>
}

impl Trade {
    fn from(action: &String, symbol: &String, price: f64, filled_oid: i32, filled_uid: i32, filler_oid: i32, filler_uid: i32, exchanged: i32) -> Self {
        Trade {
            action: action.clone(),
            symbol: symbol.clone(),
            price,
            filled_oid,
            filled_uid,
            filler_oid,
            filler_uid,
            exchanged,
            execution_time: Utc::now()
        }
    }

    // Create a Trade from a pair of Orders.
    pub fn order_to_trade(pending: &Order, filler: &Order, exchanged: i32) -> Self {
        Trade::from(&pending.action, &pending.symbol, pending.price, pending.order_id, pending.user_id.unwrap(), filler.order_id, filler.user_id.unwrap(), exchanged)
    }

    /* Used when reading data directly from the database. */
    pub fn direct(symbol: &str, action: &str, price: f64, filled_oid: i32, filled_uid: i32, filler_oid: i32, filler_uid: i32, exchanged: i32, execution_time: DateTime<Utc>) -> Self {
        Trade {
            symbol: symbol.to_string().clone(),
            action: action.to_string().clone(),
            price,
            filled_oid,
            filled_uid,
            filler_oid,
            filler_uid,
            exchanged,
            execution_time
        }
    }
}

impl Clone for Trade {
    fn clone(&self) -> Self {
        Trade {
            action: self.action.clone(),
            symbol: self.symbol.clone(),
            ..*self
        }
    }
}
