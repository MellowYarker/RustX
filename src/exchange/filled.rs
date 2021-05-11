// use crate::exchange::requests::Order;
use crate::exchange::Order;

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
