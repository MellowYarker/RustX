use std::collections::BinaryHeap;
use std::cmp::Reverse;
use crate::exchange::{Order, Trade, OrderStatus};

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
     *
     * If orders are completely or partial filled, turn them into Trades and add them
     * to the trades vector.
     *
     * Returns the lowest sell price that was filled or None if no trade occured.
     */
    pub fn fill_buy_order(&mut self, highest_bid: &mut Order, trades: &mut Vec<Trade>, modified_orders: &mut Vec<Order>) -> Option<f64> {

        // No trades by default
        let mut new_price = None;

        // Loop until no more orders can be filled.
        loop {
            // The new buy order was filled.
            if highest_bid.quantity == highest_bid.filled {
                highest_bid.status = OrderStatus::COMPLETE;
                break;
            }

            // We try to fill the lowest sell
            // peek is less expensive than pop
            let lowest_offer = match self.sell_orders.peek() {
                Some(bid) => &bid.0,
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
                    let mut lowest_offer = self.sell_orders.pop().unwrap();
                    lowest_offer.0.filled += amount_traded;
                    lowest_offer.0.status = OrderStatus::COMPLETE;

                    // Add this trade
                    highest_bid.filled += amount_traded;
                    trades.push(Trade::order_to_trade(&lowest_offer.0, &highest_bid, amount_traded));
                    modified_orders.push(lowest_offer.0.clone());
                } else {
                    // The buy order was completely filled.
                    let amount_traded = highest_bid_remaining;

                    // Update the lowest offer
                    let mut lowest_offer = &mut (self.sell_orders.peek_mut().unwrap().0);
                    lowest_offer.filled += amount_traded;

                    // Newly placed order was filled
                    highest_bid.filled += amount_traded;
                    trades.push(Trade::order_to_trade(&lowest_offer, &highest_bid, amount_traded));
                    modified_orders.push(lowest_offer.clone());
                }
            } else {
                // Highest buy doesn't reach lowest sell.
                break;
            }
        }

        return new_price;
    }

    /* Given a sell order, try to fill it with existing buy orders in the market.
     *
     * If orders are completely or partial filled, turn them into Trades and add them
     * to the trades vector.
     *
     * Returns the highest buy price that was filled or None if no trade occured.
    */
    pub fn fill_sell_order(&mut self, lowest_offer: &mut Order, trades: &mut Vec<Trade>, modified_orders: &mut Vec<Order>) -> Option<f64> {
        // No trades by default
        let mut new_price = None;

        // Loop until no more orders can be filled.
        loop {
            // The new sell order was filled.
            if lowest_offer.quantity == lowest_offer.filled {
                lowest_offer.status = OrderStatus::COMPLETE;
                break;
            }

            // We try to fill the highest buy
            // peek is less expensive than pop.
            let highest_bid = match self.buy_orders.peek() {
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
                    let mut highest_bid = self.buy_orders.pop().unwrap();
                    highest_bid.filled += amount_traded;
                    highest_bid.status = OrderStatus::COMPLETE;

                    lowest_offer.filled += amount_traded;

                    // Add the updated buy to the Vectors we return
                    trades.push(Trade::order_to_trade(&highest_bid, &lowest_offer, amount_traded));
                    modified_orders.push(highest_bid.clone());
                } else {
                    // The sell order was completely filled.
                    let amount_traded = lowest_sell_remaining;

                    // Update the highest bid.
                    let mut highest_bid = self.buy_orders.peek_mut().unwrap();
                    highest_bid.filled += amount_traded;

                    // Newly placed order was filled
                    lowest_offer.filled += amount_traded;

                    trades.push(Trade::order_to_trade(&highest_bid, &lowest_offer, amount_traded));
                    modified_orders.push(highest_bid.clone());
                }
            } else {
                // Lowest sell doesn't reach highest buy.
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
    pub fn fill_existing_orders(&mut self, order: &mut Order) -> Option<(Vec<Order>, Vec<Trade>)> {
        // We will populate this if any orders get filled.
        let mut trades: Vec<Trade> = Vec::new();
        let mut modified_orders: Vec<Order> = Vec::new();

        let mut new_price = None;
        match &order.action[..] {
            // New buy order, try to fill some existing sells
            "BUY" => {
                new_price = self.fill_buy_order(order, &mut trades, &mut modified_orders);
            },
            // New sell order, try to fill some existing buys
            "SELL" => {
                new_price = self.fill_sell_order(order, &mut trades, &mut modified_orders);
            },
            _ => () // Not possible
        }

        // Update the market stats as the state has changed.
        match new_price {
            // Price change means orders were filled
            Some(_) => {
                return Some((modified_orders, trades));
            },
            None => return None
        }
    }
}
