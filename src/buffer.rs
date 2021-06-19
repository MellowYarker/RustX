use std::collections::HashMap;
use std::collections::hash_map;
use std::vec;
use std::convert::TryInto;

use chrono::{Local, DateTime};

use crate::exchange::{OrderStatus, Trade, Order};


// TODO: We want several data structures, maybe inside one "Buffer" struct
//       where we can store:
//       1. Orders as follows
//           {
//             action: Option<String>,
//             symbol: Option<String>,
//             quantity: Option<i32>,
//             filled: Option<i32>,
//             price: Option<f64>,
//             order_id: Option<i32>,
//             status: Option<OrderStatus>,
//             user_id: Option<i32>
//           }
//
//          By storing orders in this way, we can represent a NEW order (i.e one the database has
//          never seen) by having some fields like action or symbol be Some(string). Meanwhile, an
//          old order would have action, symbol, quantity, price, order_id, user_id all set to None
//          in our Buffer structure, since the database knows the values already and they do not
//          change.
//
//          Therefore, orders the DB knows about will be stored in the buffer as:
//           {
//             action: None,
//             symbol: None,
//             quantity: None,
//             filled: Option<i32>, // Some(i32) if update occured, else None
//             price: None,
//             order_id: None,
//             status: Option<OrderStatus>, // Some(OrderStatus) if update occured, else None
//             user_id: None
//           }
//          This makes it easy for us to determine
//          a. If we need to to an insert (new order) or update (old order).
//          b. Which fields need to be updated for an old order.
//          c. We can differentiate between a complete order, a cancelled order, and a pending
//          order.
//
//      2. Trades can just be stored in a vector. They are always new and unique!
//      3. New accounts can probably still be inserted immediately
//      4. The "exchangeStats", i.e max order ID, can just be read straight from the exchange
//
//
//  Ideally, we will spin up some thread whenever we want to perform DB writes so this doesn't
//  affect program operation. This means we want to move the buffers to the other thread,
//  essentially draining them in the main thread (not deallocating!).
//
//  Somehow, we should run this all in a loop, i.e every n seconds, write the buffer to the DB. Or
//  instead, once the buffer reaches a certain size (10MB?), write the contents to the DB. It
//  really depends on:
//
//      1. Overall program memory consumption
//          - If the program is running hot, we will have to decrease the buffer capacity.
//            However, we can probably control a few things like # users in the cache (evict LRU),
//            # of pending orders we want to store in each market, etc.
//      2. Latency of writing to the database.
//      3. Ability to write to DB by moving buffer to another thread and continuing normal
//         operations (in this case, we could have a very large buffer).

/* This struct represents an order that is ready to be written to the database.
 * We make the following distinction between known, and unknown orders:
 *
 *  Known Orders are orders that are known to the database, they have already been written to disk before.
 *      -   If a known order is to be updated, the only fields that might have Some(val) are
 *          filled, status, and update_time.
 *
 *  Unknown Orders are orders that are not known to the database, they have been placed for the
 *  first time.
 *      -   Unknown orders are to be *inserted*, and ALL of their fields will have values,
 *          excluding potentially time_updated.
 * */
#[derive(Debug)]
pub struct DatabaseReadyOrder {
    action:       Option<String>,
    symbol:       Option<String>,
    quantity:     Option<i32>,
    filled:       Option<i32>,
    price:        Option<f64>,
    order_id:     Option<i32>,
    status:       Option<OrderStatus>,
    user_id:      Option<i32>,
    time_placed:  Option<DateTime<Local>>,
    time_updated: Option<DateTime<Local>>,
}

impl DatabaseReadyOrder {

    fn new() -> Self {
        DatabaseReadyOrder {
            action:       None,
            symbol:       None,
            quantity:     None,
            filled:       None,
            price:        None,
            order_id:     None,
            status:       None,
            user_id:      None,
            time_placed:  None,
            time_updated: None,
        }
    }

    /* Creates an order ready to be inserted to the database.
     * Note that we take the current time for time_placed.
     **/
    fn prepare_new_order(order: &Order) -> Self {
        DatabaseReadyOrder {
            action: Some(order.action.clone()),
            symbol: Some(order.symbol.clone()),
            quantity: Some(order.quantity),
            filled: Some(order.filled),
            price: Some(order.price),
            order_id: Some(order.order_id),
            status: Some(order.status),
            user_id: order.user_id,
            time_placed:  Some(Local::now()),
            time_updated: None,
        }
    }

    /* Update the DatabaseReadyOrder given the current order's state. */
    fn update_ready_order(&mut self, order: &Order, update_filled: bool) {

        if update_filled {
            self.filled = Some(order.filled);
        }

        match order.status {
            // We only store pending orders (excluding buffers),
            // so DB would know about pending (i.e ignore it).
            OrderStatus::PENDING => (),
            OrderStatus::COMPLETE | OrderStatus::CANCELLED => self.status = Some(order.status)
        }

        self.time_updated = Some(Local::now());
    }

    fn update_filled(&mut self, new_filled: i32) {
        self.filled = Some(new_filled);
    }

    fn update_status(&mut self, new_status: OrderStatus) {
        self.status = Some(new_status);
    }

    fn update_time_updated(&mut self) {
        self.time_updated = Some(Local::now());
    }
}

#[derive(Debug)]
pub enum BufferState {
    EMPTY,
    NONEMPTY,
    FULL
}

// TODO: What do we do if to fulfill an order, we have to go over the buffer's capacity?
//       a) Empty the buffer well before it's at < 100% capacity
//       b) Empty the buffer the moment we hit 100%, potentially stalling the main thread
//       c) Increase the capacity of the buffer temporarily?
#[derive(Debug)]
pub struct OrderBuffer {
    data: HashMap<i32, DatabaseReadyOrder>,
    state: BufferState
}

impl OrderBuffer {

    /* TODO: Mess around with this.
     * Capacity is the number of Orders we want to store in the buffer. */
    pub fn new(capacity: u32) -> Self {
        let data: HashMap<i32, DatabaseReadyOrder> = HashMap::with_capacity(capacity.try_into().unwrap());
        let state = BufferState::EMPTY;
        OrderBuffer {
            data,
            state
        }
    }

    /* Gives us access to the internal data buffer. */
    pub fn drain_buffer(&mut self) -> hash_map::Drain<'_, i32, DatabaseReadyOrder> {
        match self.state {
            BufferState::EMPTY => eprintln!("The buffer is empty, there is nothing to drain."),
            BufferState::NONEMPTY => eprintln!("The buffer is not full, we can wait before draining."),
            BufferState::FULL => ()
        }
        self.state = BufferState::EMPTY;
        self.data.drain()
    }

    /* We want to empty the buffer before it's close to
     * being completely full. This prevents us from
     * dealing with a situation where the buffer gets
     * full in the middle of processing an order.
     **/
    pub fn update_space_remaining(&mut self) {
        // If we've used 90% or more of the buffer, update the state.
        let used: f64 = self.data.len() as f64;
        let max : f64 = self.data.capacity() as f64;
        if 0.9 < (used / max) {
            self.state = BufferState::FULL;
        }
    }

    /* Note that "unknown" doesn't mean unknown to the buffer.
     * Rather, it means the order is unknown to the database.
     *
     * This function is meant for newly placed orders.
     **/
    pub fn add_unknown_to_order_buffer(&mut self, order: &Order) {
        match self.state {
            BufferState::FULL => panic!("Attempting to write an unknown order to a full buffer!"),
            BufferState::EMPTY => self.state = BufferState::NONEMPTY,
            _ => ()
        }

        match self.data.insert(order.order_id, DatabaseReadyOrder::prepare_new_order(order)) {
            Some(_) => {
                panic!("\
    Order added to OrderBuffer was already stored in OrderBuffer!\
    Find where add_unknown_to_order_buffer is called, and make sure to only add newly submitted orders!")
            },
            None => ()
        }
    }

    /* If we're calling this function, the order has clearly been updated, and since we store
     * status in the order now, we can clearly check if an order's status is changed from
     * pending.
     **/
    pub fn add_or_update_entry_in_order_buffer(&mut self, order: &Order, update_filled: bool) {
        // TODO: we should use vacant/occupied, then on vacant, check buffer state for FULL.
        let entry = self.data.entry(order.order_id).or_insert(DatabaseReadyOrder::new());
        entry.update_ready_order(order, update_filled);

        if let BufferState::EMPTY = self.state {
            self.state = BufferState::NONEMPTY;
        }
    }

}

#[derive(Debug)]
pub struct TradeBuffer {
    // TODO: We also need to include execution_time!
    //       Should we add that to the Trade struct, or append it some other way?
    data: Vec<Trade>, // A simple vector that stores the trades in the order they occur.
    state: BufferState
}

impl TradeBuffer {

    pub fn new(capacity: u32) -> Self {
        let data: Vec<Trade> = Vec::with_capacity(capacity.try_into().unwrap());
        let state = BufferState::EMPTY;
        TradeBuffer {
            data,
            state
        }
    }

    /* We want to empty the buffer before it's close to
     * being completely full. This prevents us from
     * dealing with a situation where the buffer gets
     * full in the middle of processing an order.
     **/
    pub fn update_space_remaining(&mut self) {
        // If we've used 90% or more of the buffer, update the state.
        let used: f64 = self.data.len() as f64;
        let max : f64 = self.data.capacity() as f64;
        if 0.9 < (used / max) {
            self.state = BufferState::FULL;
        }
    }

    /* Call this when we want to consume the buffer and write it to the database. */
    pub fn drain_buffer(&mut self) -> vec::Drain<'_, Trade> {
        match self.state {
            BufferState::EMPTY => eprintln!("The buffer is empty, there is nothing to drain."),
            BufferState::NONEMPTY => eprintln!("The buffer is not full, we can wait before draining."),
            BufferState::FULL => ()
        }
        self.state = BufferState::EMPTY;
        self.data.drain(..)
    }

    pub fn add_trade_to_buffer(&mut self, trade: Trade) {
        match self.state {
            BufferState::FULL => panic!("Attempting to write a trade to a full buffer!"),
            BufferState::EMPTY => self.state = BufferState::NONEMPTY,
            _ => ()
        }
        self.data.push(trade);
    }

    /* This will consume the trades vector. */
    pub fn add_trades_to_buffer(&mut self, trades: &mut Vec<Trade>) {
        match self.state {
            BufferState::FULL => panic!("Attempting to write several trades to a full buffer!"),
            BufferState::EMPTY => self.state = BufferState::NONEMPTY,
            _ => ()
        }
        self.data.append(trades);
    }
}

#[derive(Debug)]
pub struct BufferCollection {
    pub buffered_orders: OrderBuffer, // where we temporarily store order updates that will be inserted/updated to the DB.
    pub buffered_trades: TradeBuffer  // where we temporarily store trades that will be inserted in the DB
}

impl BufferCollection {
    pub fn new(order_buffer_cap: u32, trade_buffer_cap: u32) -> Self {
        let buffered_orders: OrderBuffer = OrderBuffer::new(order_buffer_cap);
        let buffered_trades: TradeBuffer = TradeBuffer::new(trade_buffer_cap);

        BufferCollection {
            buffered_orders,
            buffered_trades
        }
    }

    // TODO:
    //  Check the remaining space of our buffers.
    //  We should probably return a struct that informs the caller
    //  whether 1 or more buffers are full.
    pub fn update_buffer_states(&mut self) {
        self.buffered_orders.update_space_remaining();
        self.buffered_trades.update_space_remaining();

        if let BufferState::FULL = self.buffered_orders.state {
            // TODO: must drain orders buffer!
            eprintln!("WARNING: order buffer is full. Write to the database!");
            self.buffered_orders.drain_buffer();
        };

        if let BufferState::FULL = self.buffered_trades.state {
            // TODO: must drain trades buffer!
            eprintln!("WARNING: trade buffer is full. Write to the database!");
            self.buffered_trades.drain_buffer();
        };
    }
}
