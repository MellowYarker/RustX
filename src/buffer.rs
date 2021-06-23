use std::collections::HashMap;
use std::collections::hash_map;
use std::vec;
use std::convert::TryInto;

use chrono::{Local, DateTime};

use postgres::Client;
use crate::database;

use crate::exchange::{Exchange, OrderStatus, Trade, Order};
use crate::exchange::stats::SecStat;


/* Just some notes.
 *
 *      1. New accounts can probably still be inserted immediately
 *      2. The "exchangeStats", i.e max order ID, can just be read straight from the exchange
 *      3. Not sure about Markets just yet... probably just have a field in each market
 *         that informs us if it has been modified since the last write!
 *
 *  Ideally, we will spin up some thread whenever we want to perform DB writes so this doesn't
 *  affect program operation. This means we want to move the buffers to the other thread,
 *  essentially draining them in the main thread (not deallocating!).
 *
 *  Somehow, we should run this all in a loop, i.e every n seconds, write the buffer to the DB. Or
 *  instead, once the buffer reaches a certain size (10MB?), write the contents to the DB. It
 *  really depends on:
 *
 *      1. Overall program memory consumption
 *          - If the program is running hot, we will have to decrease the buffer capacity.
 *            However, we can probably control a few things like # users in the cache (evict LRU),
 *            # of pending orders we want to store in each market, etc.
 *      2. Latency of writing to the database.
 *      3. Ability to write to DB by moving buffer to another thread and continuing normal
 *         operations (in this case, we could have a very large buffer).
 **/

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

/* TODO: Do we want to do Trades here too?
 * This struct helps us categorize which tables
 * are to be modified given the current Orders buffer.*/
#[derive(Debug)]
pub struct TableModCategories {
    insert_orders: Vec<DatabaseReadyOrder>,
    update_orders: Vec<DatabaseReadyOrder>,
    insert_pending: Vec<i32>,
    delete_pending: Vec<i32>,
    update_markets: HashMap<String, ()> // Just store symbols of modified markets
}

impl TableModCategories {
    pub fn new() -> Self {
        let update_orders  = Vec::new();
        let delete_pending = Vec::new();
        let update_markets = HashMap::new();

        TableModCategories {
            insert_orders: update_orders.clone(),
            update_orders,
            insert_pending: delete_pending.clone(),
            delete_pending,
            update_markets
        }
    }
}

#[derive(Debug)]
pub enum BufferState {
    EMPTY,
    NONEMPTY,
    FULL
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

    /* Gives us access to the internal data buffer. */
    pub fn drain_buffer(&mut self) -> hash_map::Drain<'_, i32, DatabaseReadyOrder> {
        match self.state {
            BufferState::EMPTY => eprintln!("The Order buffer is empty, there is nothing to drain."),
            BufferState::NONEMPTY => eprintln!("The Order buffer is not full, we can wait before draining."),
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

    fn prepare_for_db_update(&mut self, categorize: &mut TableModCategories) {
        // TODO: Create a TableModCategories struct, fill it by iterating over the HashMap
        //       and assigning DatabaseReadyOrder's to the appropriate fields vecs.
        //
        // TODO: If we want to decrease redundant computation, and increase redundant data
        // replication, we can store Some(symbol) in ALL DatabaseReadyOrder's, then use the
        // update_market field of TableModCategories.
        for (id, order) in self.data.iter_mut() {
            match order.order_id {
                // Unknown order
                // care about insert pending, insert order
                Some(_) => {
                    categorize.insert_orders.push(order.clone());

                    if let OrderStatus::PENDING = order.status.unwrap() {
                        categorize.insert_pending.push(order.order_id.unwrap().clone());
                    }
                },
                // Known order
                // care about delete pending, update order
                None => {
                    // First, add the order ID.
                    order.order_id = Some(id.clone());
                    categorize.update_orders.push(order.clone());

                    // If cancelled/complete
                    if let Some(_) = order.status {
                        categorize.delete_pending.push(order.order_id.unwrap().clone());
                    }
                }
            }
        }
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

    /* Call this when we want to consume the buffer and write it to the database. */
    pub fn drain_buffer(&mut self) -> vec::Drain<'_, Trade> {
        match self.state {
            BufferState::EMPTY => eprintln!("The trade buffer is empty, there is nothing to drain."),
            BufferState::NONEMPTY => eprintln!("The trade buffer is not full, we can wait before draining."),
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

    pub fn flush_on_shutdown(&mut self, exchange: &Exchange, conn: &mut Client) {
        self.buffered_orders.update_space_remaining();
        self.buffered_trades.update_space_remaining();

        // Flush Orders
        let mut categories = TableModCategories::new();
        self.buffered_orders.prepare_for_db_update(&mut categories);
        BufferCollection::launch_batch_db_updates(&categories, exchange, conn);
        self.buffered_orders.drain_buffer();

        // Flush Trades
        database::insert_buffered_trades(&self.buffered_trades.data, conn);
        self.buffered_trades.drain_buffer();
    }

    /* Check our buffer states.
     * Returns true if Orders buffer was drained, false otherwise.
     *      - If order buffer drained, we can reset user modified fields.
     **/
    pub fn update_buffer_states(&mut self, exchange: &Exchange, conn: &mut Client) -> bool {
        self.buffered_orders.update_space_remaining();
        self.buffered_trades.update_space_remaining();

        let mut orders_drained = false;

        if let BufferState::FULL = self.buffered_orders.state {
            eprintln!("WARNING: order buffer is full. Write to the database!");

            // Prepare for Order buffer drain
            let mut categories = TableModCategories::new();
            self.buffered_orders.prepare_for_db_update(&mut categories);
            BufferCollection::launch_batch_db_updates(&categories, exchange, conn);

            self.buffered_orders.drain_buffer();
            orders_drained = true;
        };

        if let BufferState::FULL = self.buffered_trades.state {
            eprintln!("WARNING: trade buffer is full. Write to the database!");
            // -------------------------------------------------------------------
            // Don't like this, but we have to insert orders before trades bc
            // trades have a foreign key constraint on order_id.
            let mut categories = TableModCategories::new();
            self.buffered_orders.prepare_for_db_update(&mut categories);
            BufferCollection::launch_batch_db_updates(&categories, exchange, conn);

            self.buffered_orders.drain_buffer();
            // -------------------------------------------------------------------

            database::insert_buffered_trades(&self.buffered_trades.data, conn);
            self.buffered_trades.drain_buffer();
        };

        return orders_drained;
    }

    fn launch_batch_db_updates(categories: &TableModCategories, exchange: &Exchange, conn: &mut Client) {
        // This has to run first, since other tables have a foreign key constraint
        // on this table's order_id field.

        // TODO: Would it decrease insert time to sort the insert_orders?
        BufferCollection::launch_insert_orders(&categories.insert_orders, conn);

        // TODO: Run these in separate threads
        BufferCollection::launch_update_orders(&categories.update_orders, conn);
        BufferCollection::launch_insert_pending_orders(&categories.insert_pending, conn);
        BufferCollection::launch_delete_pending_orders(&categories.delete_pending, conn);

        BufferCollection::launch_exchange_stats_update(exchange.total_orders, conn);

        // TODO: We can decrease the computation time for this, see comment
        //       in prepare_for_db_update.
        BufferCollection::launch_update_market(&exchange.statistics, conn);
    }

    /* Entry point for batch inserting unknown orders to database */
    fn launch_insert_orders(orders_to_insert: &Vec<DatabaseReadyOrder>, conn: &mut Client) {
        database::insert_buffered_orders(orders_to_insert, conn);
    }

    /* Entry point for batch updating known orders in database */
    fn launch_update_orders(orders_to_update: &Vec<DatabaseReadyOrder>, conn: &mut Client) {
        database::update_buffered_orders(orders_to_update, conn);
    }

    /* Entry point for batch inserting pending orders for unknown Orders to database  */
    fn launch_insert_pending_orders(pending_to_insert: &Vec<i32>, conn: &mut Client) {
        database::insert_buffered_pending(pending_to_insert, conn);
    }

    /* Entry point for batch deleting pending orders from database  */
    fn launch_delete_pending_orders(pending_to_delete: &Vec<i32>, conn: &mut Client) {
        database::delete_buffered_pending(pending_to_delete, conn);
    }

    /* Entry point for batch market stats updates. */
    fn launch_exchange_stats_update(total_orders: i32, conn: &mut Client) {
        database::update_total_orders(total_orders, conn);
    }

    /* Entry point for batch updating market stats in database  */
    fn launch_update_market(markets: &HashMap<String, SecStat>, conn: &mut Client) {
        // Create iterator of modified SecStat's and pass that to DB api.
        let updated_markets: Vec<&SecStat> = markets.values().filter(|market| market.modified == true).collect();
        database::update_buffered_markets(&updated_markets, conn);
    }
}
