use crate::exchange::Order;

#[derive(Debug)]
pub struct FilledOrder {
    pub action: String,
    pub security: String,
    pub price: f64,         // price at which this order was filled
    pub id: i32,            // this order's ID
    pub username: String,
    pub filled_by: i32,     // the order ID that filled this order
    pub filler_name: String,
    pub exchanged: i32      // the amount of shares exchanged
}

impl FilledOrder {
    fn from(action: &String, security: &String, price: f64, id: i32, username: &String, filled_by: i32, filler_name: &String, exchanged: i32) -> Self {
        FilledOrder {
            action: action.clone(),
            security: security.clone(),
            price,
            id,
            username: username.clone(),
            filled_by,
            filler_name: filler_name.clone(),
            exchanged
        }
    }

    // Create a FilledOrder from a pair of orders.
    pub fn order_to_filled_order(old: &Order, filler: &Order, exchanged: i32) -> Self {
        FilledOrder::from(&old.action, &old.security, old.price, old.order_id, &old.username, filler.order_id, &filler.username, exchanged)
    }
}

impl Clone for FilledOrder {
    fn clone(&self) -> Self {
        FilledOrder {
            action: self.action.clone(),
            security: self.security.clone(),
            username: self.username.clone(),
            filler_name: self.filler_name.clone(),
            ..*self
        }
    }
}
