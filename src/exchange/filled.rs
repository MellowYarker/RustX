use crate::exchange::Order;

/*
 * TODO: Perhaps a more fitting name is "Trade".
 *       FilledOrder might imply the entire order is filled.
 *       Trade indicates that it might be *partially* filled.
 **/
#[derive(Debug)]
pub struct FilledOrder {
    pub action: String,
    pub security: String,
    pub price: f64,         // price at which this trade was occured
    pub filled_oid: i32,    // ID of order getting filled
    pub filled_uid: i32,    // ID of user who placed the order that is being filled
    pub filler_oid: i32,    // ID of new order that triggered the trade
    pub filler_uid: i32,    // ID of user who placed new order that triggered the trade
    pub exchanged: i32      // the amount of shares exchanged
}

impl FilledOrder {
    fn from(action: &String, security: &String, price: f64, filled_oid: i32, filled_uid: i32, filler_oid: i32, filler_uid: i32, exchanged: i32) -> Self {
        FilledOrder {
            action: action.clone(),
            security: security.clone(),
            price,
            filled_oid,
            filled_uid,
            filler_oid,
            filler_uid,
            exchanged
        }
    }

    // Create a FilledOrder from a pair of Orders.
    pub fn order_to_filled_order(pending: &Order, filler: &Order, exchanged: i32) -> Self {
        FilledOrder::from(&pending.action, &pending.security, pending.price, pending.order_id, pending.user_id.unwrap(), filler.order_id, filler.user_id.unwrap(), exchanged)
    }

    /* Used when reading data directly from the database. */
    pub fn direct(symbol: &str, action: &str, price: f64, filled_oid: i32, filled_uid: i32, filler_oid: i32, filler_uid: i32, exchanged: i32) -> Self {
        FilledOrder {
            security: symbol.to_string().clone(),
            action: action.to_string().clone(),
            price,
            filled_oid,
            filled_uid,
            filler_oid,
            filler_uid,
            exchanged
        }
    }
}

impl Clone for FilledOrder {
    fn clone(&self) -> Self {
        FilledOrder {
            action: self.action.clone(),
            security: self.security.clone(),
            ..*self
        }
    }
}
