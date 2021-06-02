use crate::exchange::Order;

#[derive(Debug)]
pub struct FilledOrder {
    pub action: String,
    pub security: String,
    pub price: f64,         // price at which this order was filled
    pub id: i32,            // this order's ID
    // pub username: String,
    pub user_id: i32,
    pub filled_by: i32,     // the order ID that filled this order
    // pub filler_name: String,
    pub filler_id: i32,
    pub exchanged: i32      // the amount of shares exchanged
}

impl FilledOrder {
    // fn from(action: &String, security: &String, price: f64, id: i32, username: &String, filled_by: i32, filler_name: &String, exchanged: i32) -> Self {
    fn from(action: &String, security: &String, price: f64, id: i32, user_id: i32, filled_by: i32, filler_id: i32, exchanged: i32) -> Self {
        FilledOrder {
            action: action.clone(),
            security: security.clone(),
            price,
            id,
            // username: username.clone(),
            user_id,
            filled_by,
            // filler_name: filler_name.clone(),
            filler_id,
            exchanged
        }
    }

    // Create a FilledOrder from a pair of orders.
    pub fn order_to_filled_order(old: &Order, filler: &Order, exchanged: i32) -> Self {
        // FilledOrder::from(&old.action, &old.security, old.price, old.order_id, &old.username, filler.order_id, &filler.username, exchanged)
        FilledOrder::from(&old.action, &old.security, old.price, old.order_id, old.user_id.unwrap(), filler.order_id, filler.user_id.unwrap(), exchanged)
    }
}

impl Clone for FilledOrder {
    fn clone(&self) -> Self {
        FilledOrder {
            action: self.action.clone(),
            security: self.security.clone(),
            // username: self.username.clone(),
            // filler_name: self.filler_name.clone(),
            ..*self
        }
    }
}
