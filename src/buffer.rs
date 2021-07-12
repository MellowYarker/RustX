use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::convert::TryInto;
use std::sync::mpsc;

use chrono::{Local, DateTime};

use postgres::Client;
use crate::database;

use crate::exchange::{Exchange, OrderStatus, Trade, Order};
use crate::exchange::stats::SecStat;

use crate::{WorkerThreads, Category};


/* This struct represents an order that is ready to be written to the database.
 * We make the following distinction between known, and unknown orders:
 *
 *  Known Orders: orders that are known to the database, they have already been written to disk before.
 *      -   If a known order is to be updated, the only fields that might have Some(val) are
 *          filled, status, and update_time.
 *      -   If the programs state of a field is the same as the database, we represent it as None here.
 *
 *  Unknown Orders: orders that are not known to the database, they have been placed for the first time.
 *      -   Unknown orders are to be *inserted*, and ALL of their fields will have values,
 *          excluding potentially time_updated.
 *
 *  The following SQL statements will need to be supported:
 *      1. Insert to Orders (Unknown Order)
 *      2. Insert to Pending (Unknown order)
 *      3. Remove from Pending (Unknown order cancelled/completed)
 *      4. Update Orders (Known order updated)
 *
 *  This struct summarizes all changes made to an order since the last write.
 *  It's effectively a DIFF.
 **/
#[derive(Debug, Clone)]
pub struct DatabaseReadyOrder {
    pub action:       Option<String>,
    pub symbol:       Option<String>,
    pub quantity:     Option<i32>,
    pub filled:       Option<i32>,
    pub price:        Option<f64>,
    pub order_id:     Option<i32>,
    pub status:       Option<OrderStatus>,
    pub user_id:      Option<i32>,
    pub time_placed:  Option<DateTime<Local>>,
    pub time_updated: Option<DateTime<Local>>,
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
}

#[derive(Debug)]
pub struct UpdateCategories {
    pub insert_orders: Vec<DatabaseReadyOrder>,
    pub update_orders: Vec<DatabaseReadyOrder>,
    pub total_orders: i32,
    pub insert_pending: Vec<i32>,
    pub delete_pending: Vec<i32>,
    pub markets_modified: HashMap<String, ()>, // Just store symbols of modified markets
    pub insert_trades: Vec<Trade>,
    pub update_markets: Vec<SecStat>
}

impl UpdateCategories {
    pub fn new() -> Self {
        let update_orders  = Vec::new();
        let delete_pending = Vec::new();
        let markets_modified = HashMap::new();
        let insert_trades = Vec::new();
        let update_markets = Vec::new();

        UpdateCategories {
            insert_orders: update_orders.clone(),
            update_orders,
            insert_pending: delete_pending.clone(),
            delete_pending,
            total_orders: 0,
            markets_modified,
            insert_trades,
            update_markets
        }
    }
}

#[derive(Debug)]
pub enum BufferState {
    EMPTY,
    NONEMPTY,
    FULL,
    FORCEFLUSH, // when an external module like user cache is full, it sets state to FORCEFLUSH
}

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

    /* This function clears the OrderBuffer.
     * I think it would be more "Rust-like" to actually call drain()
     * on the data, returning an iterator for use, but this works so...
     **/
    pub fn drain_buffer(&mut self) {
        match self.state {
            BufferState::EMPTY => println!("The Order buffer is empty, there is nothing to drain."),
            BufferState::NONEMPTY => println!("The Order buffer was not full, we could have waited before draining."),
            BufferState::FORCEFLUSH => println!("The Order buffer was forced to flush."),
            BufferState::FULL => ()
        }
        self.state = BufferState::EMPTY;
        self.data.clear();
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
            BufferState::FULL => panic!("Attempted to write an unknown order to a full buffer!"),
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
        let entry = match self.data.entry(order.order_id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                if let BufferState::FULL = self.state {
                    panic!("\
Attempted to add an order to a full Order buffer!
Be sure to clear the buffer well before it reaches capacity!");
                }
                entry.insert(DatabaseReadyOrder::new())
            }
        };

        entry.update_ready_order(order, update_filled);

        if let BufferState::EMPTY = self.state {
            self.state = BufferState::NONEMPTY;
        };
    }

    fn prepare_for_db_update(&mut self, categories: &mut UpdateCategories, exchange: &Exchange) {
        // TODO: If we want to decrease redundant computation, and increase redundant data
        // replication, we can store Some(symbol) in ALL DatabaseReadyOrder's, then use the
        // update_market field of TableModCategories.
        for (id, order) in self.data.iter_mut() {
            match order.order_id {
                // Unknown order
                // care about insert pending, insert order
                Some(_) => {
                    categories.insert_orders.push(order.clone());

                    if let OrderStatus::PENDING = order.status.unwrap() {
                        categories.insert_pending.push(order.order_id.unwrap().clone());
                    }
                },
                // Known order
                // care about delete pending, update order
                None => {
                    // First, add the order ID.
                    order.order_id = Some(id.clone());
                    categories.update_orders.push(order.clone());

                    // If cancelled/complete
                    if let Some(_) = order.status {
                        categories.delete_pending.push(order.order_id.unwrap().clone());
                    }
                }
            }
        }
        // Create iterator of modified SecStat's and pass that to DB api.
        categories.total_orders = exchange.total_orders;
        categories.update_markets = exchange.statistics.values().cloned().filter(|market| market.modified == true).collect();
    }
}

#[derive(Debug)]
pub struct TradeBuffer {
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

    /* This function clears the TradeBuffer.
     * I think it would be more "Rust-like" to actually call drain()
     * on the data, returning an iterator for use, but this works so...
     **/
    pub fn drain_buffer(&mut self) {
        match self.state {
            BufferState::EMPTY => println!("The Trade buffer is empty, there is nothing to drain."),
            BufferState::NONEMPTY => println!("The Trade buffer was not full, we could have waited before draining."),
            BufferState::FORCEFLUSH => println!("The Trade buffer was forced to flush."),
            BufferState::FULL => ()
        }
        self.state = BufferState::EMPTY;
        self.data.clear();
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

    fn prepare_for_db_update(&mut self, categories: &mut UpdateCategories) {
        categories.insert_trades.append(&mut self.data);
    }
}

#[derive(Debug)]
pub struct BufferCollection {
    pub buffered_orders: OrderBuffer, // where we temporarily store order updates that will be inserted/updated to the DB.
    pub buffered_trades: TradeBuffer, // where we temporarily store trades that will be inserted in the DB
    pub tx: Option<mpsc::Sender<Option<UpdateCategories>>> // Transmitter to thread that writes to the database
}

impl BufferCollection {
    pub fn new(order_buffer_cap: u32, trade_buffer_cap: u32) -> Self {
        let buffered_orders: OrderBuffer = OrderBuffer::new(order_buffer_cap);
        let buffered_trades: TradeBuffer = TradeBuffer::new(trade_buffer_cap);

        BufferCollection {
            buffered_orders,
            buffered_trades,
            tx: None
        }
    }

    // No, we don't need a function for this, but it's called once and it makes
    // it clear what's happening to the Sender.
    pub fn set_transmitter(&mut self, tx: mpsc::Sender<Option<UpdateCategories>>) {
        self.tx = Some(tx);
    }

    pub fn force_flush(&mut self, exchange: &Exchange) {
        match self.buffered_orders.state {
            BufferState::FULL |
            BufferState::NONEMPTY |
            BufferState::FORCEFLUSH => {
                self.buffered_orders.state = BufferState::FORCEFLUSH
            },
            _ => println!("Order buffer empty, nothing to flush.")
        }

        match self.buffered_trades.state {
            BufferState::FULL |
            BufferState::NONEMPTY |
            BufferState::FORCEFLUSH => {
                self.buffered_trades.state = BufferState::FORCEFLUSH
            },
            _ => println!("Trades buffer empty, nothing to flush.")
        }

        self.transmit_buffer_data(exchange);
    }

    pub fn flush_on_shutdown(&mut self, exchange: &Exchange) {
        self.update_buffer_states();

        self.force_flush(exchange);
        println!("Shutdown request has been propagated.");
    }

    /* Sends the buffer data down the channel for the other thread to handle.
     * Returns true if the order buffer was drained, false otherwise.
     **/
    pub fn transmit_buffer_data(&mut self, exchange: &Exchange) -> bool{
        let mut orders_drained = false;
        let mut pending_updates = false;
        let mut categories = UpdateCategories::new();

        match self.buffered_orders.state {
            BufferState::FULL | BufferState::FORCEFLUSH => {
                pending_updates = true;
                // Move all the Order buffer stuff into categories
                self.buffered_orders.prepare_for_db_update(&mut categories, exchange);
                self.buffered_orders.drain_buffer();
                orders_drained = true;
            },
            _ => ()
        }

        match self.buffered_trades.state {
            BufferState::FULL | BufferState::FORCEFLUSH => {
                pending_updates = true;
                // We have to insert orders before trades, since
                // trades have a foreign key constraint on order_id.
                if let BufferState::NONEMPTY = self.buffered_orders.state {
                    // Move all the Order buffer stuff into categories
                    self.buffered_orders.prepare_for_db_update(&mut categories, exchange);
                    self.buffered_orders.drain_buffer();
                    orders_drained = true;
                }

                // Move all the Trades buffer stuff into categories
                self.buffered_trades.prepare_for_db_update(&mut categories);
                self.buffered_trades.drain_buffer();
            },
            _ => ()
        }

        // Send the categories to the thread if we have updates.
        if pending_updates {
            self.tx.as_ref().unwrap().send(Some(categories)).unwrap();
        }
        return orders_drained;
    }

    /* If our buffers are close to capacity, we will update their state to full. */
    pub fn update_buffer_states(&mut self) {
        self.buffered_orders.update_space_remaining();
        self.buffered_trades.update_space_remaining();
    }

    /* This function launches the following database operations:
     *      1. Insert new orders, many tables use order_id as a FK so it must occur first.
     *      2. Update known orders
     *      3. Insert new pending orders
     *      4. Delete old pending orders
     *      5. Update total orders on exchange
     *      6. Update Markets stats.
     *      7. Insert the new trades
     *
     * We can actually run items 2-7 concurrently, we just need (1)
     * to finish first. We approach concurrent writes in the following way:
     *
     *      1. Send insert_orders to the thread that inserts new orders, wait for a response.
     *      2. Send ALL other categories to their respective threads to be inserted.
     *      3. We DO NOT need to wait for these threads to complete.
     **/
    pub fn launch_batch_db_updates<T>(categories: &UpdateCategories, workers: &mut WorkerThreads<T>) {

        // 1. Write to worker 1
        let tx = workers.channels.get(0).unwrap();
        let mut insert_container = UpdateCategories::new();
        insert_container.insert_orders = categories.insert_orders.clone();
        tx.send((insert_container, Category::INSERT_NEW)).unwrap();

        // 2. Wait for response 'true' from insert thread
        if workers.insert_orders_response.recv().unwrap() {
            // Send corresponding data to each worker thread
            // 2. update orders
            let tx = workers.channels.get(1).unwrap();
            let mut update_order_container = UpdateCategories::new();
            update_order_container.update_orders = categories.update_orders.clone();
            tx.send((update_order_container, Category::UPDATE_KNOWN)).unwrap();

            // 3. insert pending
            let tx = workers.channels.get(2).unwrap();
            let mut insert_pending_container = UpdateCategories::new();
            insert_pending_container.insert_pending = categories.insert_pending.clone();
            tx.send((insert_pending_container, Category::INSERT_PENDING)).unwrap();

            // 4. delete pending
            let tx = workers.channels.get(3).unwrap();
            let mut delete_pending_container = UpdateCategories::new();
            delete_pending_container.delete_pending = categories.delete_pending.clone();
            tx.send((delete_pending_container, Category::DELETE_PENDING)).unwrap();

            // 5. update exchange stats
            let tx = workers.channels.get(4).unwrap();
            let mut update_total_container = UpdateCategories::new();
            update_total_container.total_orders = categories.total_orders.clone();
            tx.send((update_total_container, Category::UPDATE_TOTAL)).unwrap();

            // 6. update market stats
            let tx = workers.channels.get(5).unwrap();
            let mut update_market_container = UpdateCategories::new();
            update_market_container.update_markets = categories.update_markets.clone();
            tx.send((update_market_container, Category::UPDATE_MARKET_STATS)).unwrap();

            // 7. insert new trades
            let tx = workers.channels.get(6).unwrap();
            let mut insert_trades_container = UpdateCategories::new();
            insert_trades_container.insert_trades = categories.insert_trades.clone();
            tx.send((insert_trades_container, Category::INSERT_NEW_TRADES)).unwrap();
        }
        /*
        // TODO: We can decrease the computation time for this, see comment
        //       in prepare_for_db_update.
        BufferCollection::launch_update_market(&categories.update_markets, conn);
        */
    }

    /* Entry point for batch inserting unknown orders to database */
    pub fn launch_insert_orders(orders_to_insert: &Vec<DatabaseReadyOrder>, conn: &mut Client) {
        database::insert_buffered_orders(orders_to_insert, conn);
    }

    /* Entry point for batch updating known orders in database */
    pub fn launch_update_orders(orders_to_update: &Vec<DatabaseReadyOrder>, conn: &mut Client) {
        database::update_buffered_orders(orders_to_update, conn);
    }

    /* Entry point for batch inserting pending orders for unknown Orders to database  */
    pub fn launch_insert_pending_orders(pending_to_insert: &Vec<i32>, conn: &mut Client) {
        database::insert_buffered_pending(pending_to_insert, conn);
    }

    /* Entry point for batch deleting pending orders from database  */
    pub fn launch_delete_pending_orders(pending_to_delete: &Vec<i32>, conn: &mut Client) {
        database::delete_buffered_pending(pending_to_delete, conn);
    }

    /* Entry point for batch market stats updates. */
    pub fn launch_exchange_stats_update(total_orders: i32, conn: &mut Client) {
        database::update_total_orders(total_orders, conn);
    }

    /* Entry point for batch updating market stats in database  */
    pub fn launch_update_market(update_markets: &Vec<SecStat>, conn: &mut Client) {
        database::update_buffered_markets(&update_markets, conn);
    }

    pub fn launch_insert_trades(trades_to_insert: &Vec<Trade>, conn: &mut Client) {
        database::insert_buffered_trades(trades_to_insert, conn);
    }
}
