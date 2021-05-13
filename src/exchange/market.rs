use std::collections::BinaryHeap;
use std::cmp::Reverse;
use crate::exchange::{Order, FilledOrder};

// The market for a security
#[derive(Debug)]
pub struct Market {
    pub buy_orders: BinaryHeap<Order>,
    pub sell_orders: BinaryHeap<Reverse<Order>>
}

impl Market {
    pub fn new(buy: BinaryHeap<Order>, sell: BinaryHeap<Reverse<Order>>) -> Self {
        Market {
            buy_orders: buy,
            sell_orders: sell
        }
    }

    /* Given a buy order, try to fill it with existing sell orders in the market.
     * Add any orders that are completely filled into the filled_orders vector.
     *
     * Returns the lowest sell price that was filled or None if no trade occured.
     */
    pub fn fill_buy_order(&mut self, highest_bid: &mut Order, filled_orders: &mut Vec<FilledOrder>) -> Option<f64> {

        // No trades by default
        let mut new_price = None;

        // Loop until no more orders can be filled.
        loop {
            // The new buy order was filled.
            if highest_bid.quantity == highest_bid.filled {
                break;
            }

            // We try to fill the lowest sell
            // Recall that the sell vector is sorted in descending order,
            // so the lowest offer is at the end.
            let lowest_offer = match self.sell_orders.pop() { // May potentially add back to vector if not filled.
                Some(bid) => bid.0,
                None => return new_price // No more sell orders to fill
            };

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

                    // Put the updated lowest offer back on the market
                    self.sell_orders.push(Reverse(update_lowest));
                }
            } else {
                // Highest buy doesn't reach lowest sell.
                self.sell_orders.push(Reverse(lowest_offer)); // Put the lowest sell back
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
    pub fn fill_sell_order(&mut self, lowest_offer: &mut Order, filled_orders: &mut Vec<FilledOrder>) -> Option<f64> {
        // No trades by default
        let mut new_price = None;

        // Loop until no more orders can be filled.
        loop {
            // The new sell order was filled.
            if lowest_offer.quantity == lowest_offer.filled {
                break;
            }

            // We try to fill the highest buy
            let highest_bid = match self.buy_orders.pop() { // May potentially add back to vector if not filled.
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
                    self.buy_orders.push(update_highest);

                }
            } else {
                // Lowest sell doesn't reach highest buy.
                self.buy_orders.push(highest_bid); // Put the highest bid back.
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
    pub fn fill_existing_orders(&mut self, order: &mut Order) -> Option<Vec<FilledOrder>> {
        // We will populate this if any orders get filled.
        let mut filled_orders: Vec<FilledOrder> = Vec::new();

        let mut new_price = None;
        match &order.action[..] {
            // New buy order, try to fill some existing sells
            "buy" => {
                new_price = self.fill_buy_order(order, &mut filled_orders);
            },
            // New sell order, try to fill some existing buys
            "sell" => {
                new_price = self.fill_sell_order(order, &mut filled_orders);
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
}
