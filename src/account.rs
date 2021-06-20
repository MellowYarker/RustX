use crate::exchange::requests::{Order, OrderStatus};
use crate::exchange::filled::Trade;

use std::collections::HashMap;

use postgres::{Client, NoTls};
use crate::database;

use crate::buffer::BufferCollection;

// Error types for authentication
pub enum AuthError<'a> {
    NoUser(&'a String), // Username
    BadPassword(Option<String>), // optional error msg
}

// Stores data about a user
#[derive(Debug)]
pub struct UserAccount {
    pub username: String,
    pub password: String,
    pub id: Option<i32>,
    /* Quick architecture note about pending_orders.
     * Format: { "symbol" => {"order_id" => Order} }
     * This means we can find pending orders by first
     * looking up the symbol, then the order ID.
     *  - Solves 2 problems at once
     *      1. Very easy to check if a pending order has been filled.
     *      2. Fast access to orders in each market (see validate_order function).
    **/
    pub pending_orders: HashMap<String, HashMap<i32, Order>>,   // Orders that have not been completely filled.
}

impl UserAccount {
    pub fn from(username: &String, password: &String) -> Self {
        let placed: HashMap<String, HashMap<i32, Order>> = HashMap::new();
        UserAccount {
            username: username.clone(),
            password: password.clone(),
            id: None, // We set this later
            pending_orders: placed,
        }
    }

    /* Used when reading values from database.*/
    pub fn direct(id: i32, username: &str, password: &str) -> Self {
        let placed: HashMap<String, HashMap<i32, Order>> = HashMap::new();
        UserAccount {
            username: username.to_string().clone(),
            password: password.to_string().clone(),
            id: Some(id),
            pending_orders: placed,
        }
    }

    // Sets this account's user id, and returns it.
    fn set_id(&mut self, users: &Users) -> i32 {
        self.id = Some(users.total + 1);
        return self.id.unwrap();
    }

    /*
     * Returns true if this order will not fill any pending orders placed by
     * this user. Otherwise, returns false.
     *
     * Consider the following scenario:
     *  -   user places buy order for 10 shares of X at $10/share.
     *          - the order remains on the market and is not filled.
     *  -   later, the same user places a sell order for 3 shares of X at n <= $10/share.
     *  -   the new order will fill their old order, which is probably undesirable,
     *      or even illegal.
     *
     **/
    pub fn validate_order(&self, order: &Order) -> bool {
        match self.pending_orders.get(&order.symbol) {
            // We only care about the market that `order` is being submitted to.
            Some(market) => {
                for (_, pending) in market.iter() {
                    // If this order will fill a pending order that this account placed:
                    if  (order.action.ne(&pending.action)) &&
                        ((order.action.as_str() == "BUY"  && pending.price <= order.price) ||
                        (order.action.as_str() == "SELL" && order.price <= pending.price))
                    {
                        return false;
                    }
                }
            },
            None => ()
        }
        return true;
    }

    fn check_pending_order_cache(&self, symbol: &String, id: i32) -> Option<String> {
        if let Some(market) = self.pending_orders.get(symbol) {
            if let Some(order) = market.get(&id) {
                return Some(order.action.clone()); // buy or sell
            }
        }

        return None;
    }

    /* Check if the user with the given username owns a pending order with this id.
     * If they do, return the order's action.
     * */
    pub fn user_placed_pending_order(&self, symbol: &String, id: i32, conn: &mut Client) -> Option<String> {
        match self.check_pending_order_cache(symbol, id) {
            Some(action) => return Some(action),
            /* TODO:
            * Recurrent issue: If it's not in the cache, but it IS in the db,
            *                  do we want to move it into the cache? Means we need
            *                  a mutable ref to self. Is this situation even possible?
            */
            None => {
                // Doesn't update cache.
                return database::read_match_pending_order(self.id.unwrap(), id, conn);
            }
        }
    }

    /* Removes a pending order from an account if it exists. */
    pub fn remove_order_from_account(&mut self, symbol: &String, id: i32) {
        if let Some(market) = self.pending_orders.get_mut(symbol) {
            market.remove(&id);
        }
    }
}


// Where we store all our users
// ------------------------------------------------------------------------------------------------------
// TODO:
//      We have a bit of an issue.
//
//          When we make a new account, the user doesn't know their ID.
//          The ID is generated for them later, and so in subsequent request,
//          we the user will pass their Username/Password combo to do things on our exchange.
//
//          The issue is, ideally, we would store userID in orders/trades, as that requires less
//          data to be stored in memory than a username (4 bytes vs. x bytes where x may be large).
//
//          The thing is, when we handle newly executed trades, the only information we have is
//          the userIDs associated with the orders. As it stands, we have no way of obtaining
//          account information from just a userID, since our user hashmap is stored as a
//             {username: account}
//          pair. Technically, we could create another hashmap of {userID: username} pairs, but this
//          seems silly, and also I don't want to have to maintain an additional data structure.
//
//          Another alternative (if I had a database link) would be to just do
//              SELECT * FROM PendingOrders WHERE user_id=$CURRENT_ID;
//          to get all the pending orders that belong to the user with the ID,
//          but we don't have that yet, and I think we would probably want some type of
//          in-memory cache so we have to deal with the problem anyways.
// ------------------------------------------------------------------------------------------------------
pub struct Users {
    users: HashMap<String, UserAccount>,
    // TODO: This should be an LRU cache eventually
    id_map: HashMap<i32, String>,   // maps user_id to username
    total: i32
}

impl Users {

    pub fn new() -> Self {
        let map: HashMap<String, UserAccount> = HashMap::new();
        // TODO: Eventually we want to do with capacity
        let id_map: HashMap<i32, String> = HashMap::new();
        Users {
            users: map,
            id_map: id_map,
            total: 0
        }
    }

    /* Update the total user count. */
    pub fn direct_update_total(&mut self, conn: &mut Client) {
        self.total = database::read_total_accounts(conn);
    }

    /* TODO: Some later PR, PER-6/7? We might want to buffer new accounts?
     *       If not, we could consider running this computation in a separate thread?
     *       (Although, adding a new user to the cache could be tricky... need concurrent hashmap
     *       and I'm not sure it's reasonable to want that yet.)
     * If an account with this username exists, do nothing, otherwise
     * add the account and return it's ID.
     */
    pub fn new_account(&mut self, account: UserAccount, conn: &mut Client) -> Option<i32> {
        // User is cached already
        if self.users.contains_key(&account.username) {
            return None;
        } else {

            // Check if the user exists.
            if let true = database::read_account_exists(&account.username, conn) {
                return None;
            }

            // User doesn't exist, so create a new one.
            let mut account = account;
            self.total = account.set_id(&self);

            // Insert to db
            match database::write_insert_new_account(&account, conn) {
                Ok(()) => {
                    self.cache_user(account);
                    return Some(self.total);
                },
                Err(()) => panic!("Something went wrong while inserting a new user!")
            }
        }
    }

    pub fn print_auth_error(err: AuthError) {
        match err {
            AuthError::NoUser(user) => println!("Authentication failed! User ({}) not found.", user),
            AuthError::BadPassword(message) => if let Some(msg) = message {
                eprintln!("{}", msg);
            } else {
                eprintln!("Authentication failed! Incorrect password!")
            }
        }
    }

    /* Stores a user in the programs cache. */
    fn cache_user(&mut self, account: UserAccount) {
        self.id_map.insert(account.id.unwrap(), account.username.clone());
        self.users.insert(account.username.clone(), account);
    }

    /* Checks the user cache*/
    fn auth_check_cache<'a>(&self, username: &'a String, password: & String) -> Result<(), AuthError<'a>> {
        if let Some(account) = self.users.get(username) {
            // Found user in cache
            if *password == account.password {
                return Ok(());
            }
            return Err(AuthError::BadPassword(None));
        }
        return Err(AuthError::NoUser(username));
    }

    /* If the username exists and the password is correct,
     * we return the user account.
     *
     * If the user doesn't exist, or the user exists and the
     * password is incorrect, we return an error.
     *
     * TODO: Maybe we can return some type of session token
     *       for the frontend to hold on to?
     *
     */
    pub fn authenticate<'a>(&mut self, username: &'a String, password: & String, conn: &mut Client) -> Result<&UserAccount, AuthError<'a>> {
        // First, we check our in-memory cache
        let mut cache_miss = true;
        match self.auth_check_cache(username, password) {
            Ok(()) => cache_miss = false,
            Err(e) => {
                if let AuthError::BadPassword(_) = e {
                    return Err(e);
                };
            }
        }

        // On cache miss, check the database.
        if cache_miss {
            match database::read_auth_user(username, password, conn) {
                // We got an account, move it into the cache.
                Ok(mut account) => {
                    // Since our user will be cached, and we are likely to do things with the user.
                    // We should probably make sure the in-mem pending orders are consistent w/ the database!
                    database::read_account_pending_orders(&mut account, conn);
                    self.cache_user(account);
                },
                Err(e) => return Err(e)
            }
        }

        // TODO: we call get twice if it was a cache hit.
        //       This is clearly stupid, but Rust's borrow checker is mad at me again,
        //       so I will figure this out later.
        //
        //  I believe this can be fixed by storing + accessing only 1 hashmap for a cache.
        //  Rather than taking &mut self, we can just take &mut HashMap.
        //  This will be fixed once I switch to userIDs instead of usernames.
        return Ok(self.users.get(username).unwrap());
    }

    /* Returns a reference to a user account if the user has been authenticated.
     * Panic's if the account isn't found, since the user is not in the cache.
     *
     * Note: We don't do any database lookups here. The authentication function
     * is always called right before, and that cache's the user!
     */
    pub fn get<'a>(&mut self, username: &'a String, authenticated: bool) -> Result<&UserAccount, AuthError<'a>> {
        if authenticated {
            match self.users.get(username) {
                // Cached
                Some(account) => return Ok(account),
                // In database
                None => panic!("\
Attempted to get user that was not already cached.
Be sure to call authenticate() before trying to get a reference to a user!")
            }
        }
        let err_msg = format!["Must authenticate before accessing account belonging to: ({})", username];
        return Err(AuthError::BadPassword(Some(err_msg)));
    }

    /* Returns a reference to a user account if the user has been authenticated.
     * Panic's if the account isn't found, since the user is not in the cache.
     *
     * Note: We don't do any database lookups here. The authentication function
     * is always called right before, and that cache's the user!
     */
    pub fn get_mut<'a>(&mut self, username: &'a String, authenticated: bool) -> Result<&mut UserAccount, AuthError<'a>> {
        if authenticated {
            match self.users.get_mut(username) {
                Some(account) => return Ok(account),
                None => panic!("\
Attempted to get user that was not already cached.
Be sure to call authenticate() before trying to get a reference to a user!")
            }
        }
        let err_msg = format!["Must authenticate before accessing account belonging to: ({})", username];
        return Err(AuthError::BadPassword(Some(err_msg)));
    }

    /* TODO: Some later PR. PER-6?
     *       Since we're decreasing DB operations, we actually *do* want to cache this
     *       user. The reasoning is simple: we have to trust the program state at all times.
     *       If the user isn't cached (their orders too) AND any updates to their
     *       orders are stored in the temp buffer but not the db or program data structures,
     *       then there's no way for us to know about the state of the users account.
     *
     * For internal use only.
     *
     * If the account is in the cache (active user), we return a mutable ref to the user.
     * If the account is in the database, we construct a user, get the pending orders,
     * then return the UserAccount to the calling function.
     *
     * This means we do not update the cache!
     */
    fn _get_mut(&mut self, username: &String, conn: &mut Client) -> (Option<&mut UserAccount>, Option<UserAccount>){
        match self.users.get_mut(username) {
            Some(account) => return (Some(account), None),
            None => {
                let mut account = match database::read_account(username, conn) {
                    Ok(acc) => acc,
                    Err(_) => panic!("Something went wrong while trying to get a user from the database!")
                };

                // Fill this account with the pending orders
                database::read_account_pending_orders(&mut account, conn);
                return (None, Some(account));
            }
        }
    }

    /* TODO: This is very likley outdated
     * Prints the account information of this user if:
     *  - the account exists and
     *  - the password is correct for this user
     */
    pub fn print_user(&mut self, username: &String, authenticated: bool) {
        match self.get_mut(username, authenticated) {
            Ok(account) => {
                println!("\nAccount information for user: {}", account.username);

                let mut client = Client::connect("host=localhost user=postgres dbname=mydb", NoTls)
                    .expect("Failed to connect to Database. Please ensure it is up and running.");

                // TODO: The cached pending orders are probably up to date?
                //       Don't think we need to call this.
                database::read_account_pending_orders(account, &mut client);
                let mut executed_trades: Vec<Trade> = Vec::new();
                database::read_account_executed_trades(account, &mut executed_trades, &mut client);

                println!("\n\tOrders Awaiting Execution");
                for (_, market) in account.pending_orders.iter() {
                    for (_, order) in market.iter() {
                        println!("\t\t{:?}", order);
                    }
                }
                println!("\n\tExecuted Trades");
                for order in executed_trades.iter() {
                    println!("\t\t{:?}", order);
                }
                println!("\n");
            },
            Err(e) => Users::print_auth_error(e)
        }
    }

    /* Update this users pending_orders, and the Orders table.
     * We have 2 cases to consider, as explained in update_account_orders().
     **/
    fn update_single_user(&mut self, buffers: &mut BufferCollection, id: i32, trades: &Vec<Trade>, is_filler: bool, conn: &mut Client) {
        // TODO:
        //  At some point, we want to get the username by calling some helper access function.
        //  This new function will
        //      1. Check the id_map cache
        //      2. If ID not found, check the database
        //      3. Update the id_map cache (LRU)

        let username: String = match self.id_map.get(&id) {
            Some(name) => name.clone(),
            None => {
                // Search the database for the user with this id.
                // Do not update the cache
                let result = database::read_user_by_id(id, conn);
                if let Err(_) = result {
                    panic!("Query to get user by id failed!");
                };

                result.unwrap()
            }
        };

        // If _get_mut gives us a database entry, place_holder will hold it
        // and account will refer to place_holder.
        let mut place_holder: UserAccount;
        let account: &mut UserAccount;

        // Gives either a mutable reference to cache,
        // or constructs account from db (no cache update).
        let result = self._get_mut(&username, conn);
        if let Some(acc) = result.0 {
            // Got reference to cache
            account = acc;
        } else {
            // Got user from database
            place_holder = result.1.unwrap();
            account = &mut place_holder;
        }
        // Since we can't remove entries while iterating, store the key's here.
        // We know we won't need more than trade.len() entries.
        let mut entries_to_remove: Vec<i32> = Vec::with_capacity(trades.len());

        // constant strings
        const BUY: &str = "BUY";
        const SELL: &str = "SELL";

        let market = account.pending_orders.entry(trades[0].symbol.clone()).or_insert(HashMap::new());

        // Vector of <Filled, OrderID>, will pass this to database API to structure update.
        let mut update_partial_filled_vec: Vec<(i32, i32)> = Vec::new();

        for trade in trades.iter() {
            let mut id = trade.filled_oid;
            let mut update_trade = trade.clone();

            // If this user submitted the order that was the filler,
            // we need to access the filled_by id and switch order type.
            if is_filler {
                id = trade.filler_oid;
                if trade.action.as_str() == "BUY" {
                    update_trade.action = SELL.to_string();
                } else {
                    update_trade.action = BUY.to_string();
                }
            }

            // After processing the order, move it to executed trades.
            match market.get_mut(&id) {
                Some(order) => {
                    // order completely filled
                    if trade.exchanged == (order.quantity - order.filled) {
                        // TODO: PER-5
                        //  This order is being removed from the account,
                        //  so we should update it in the DIFF buffer (hashmap).
                        //  Specifically, we should set its filled to quantity, and status to
                        //  pending.
                        order.status = OrderStatus::COMPLETE;
                        order.filled = order.quantity;
                        buffers.buffered_orders.add_or_update_entry_in_order_buffer(&order, true); // PER-5 update
                        entries_to_remove.push(order.order_id);
                    } else if !is_filler {
                        // Don't update the filler's filled count,
                        // new orders are added to accounts in submit_order_to_market.

                        // TODO: PER-5
                        //  This order is being updated, we should update the buffer (hashmap) also!
                        order.filled += trade.exchanged;
                        buffers.buffered_orders.add_or_update_entry_in_order_buffer(&order, true); // PER-5 update
                        // Extend the vector of orders we will update
                        update_partial_filled_vec.push((order.filled, order.order_id));
                    }
                },
                None => ()
            }
        }

        // Remove any completed orders from the accounts pending orders.
        for i in &entries_to_remove {
            market.remove(&i);
        }

        // TODO - Performance Opportunity:
        //      - If we can perform both these updates in parallel, i.e execute the functions in
        //        separate threads, on different DB connections, that might be a good idea!

        // TODO - PER-5
        //  We don't want to perform this pending delete, or parital update anymore.
        //  Instead, we will just perform the updates according to what is in the DIFF buffer.
        //  That is, we want to write to the DIFF buffers instead.

        // Remove all the completed orders from the database's pending table
        // and update Orders table.
        if entries_to_remove.len() > 0 {
            database::write_delete_pending_orders(&entries_to_remove, conn, OrderStatus::COMPLETE);
        }

        // For each trade that partially filled an order, update `filled` in Orders table.
        database::write_partial_update_filled_counts(&update_partial_filled_vec, conn);
    }

    /* Given a vector of Trades, update all the accounts
     * that had orders filled.
     */
    pub fn update_account_orders(&mut self, trades: &mut Vec<Trade>, buffers: &mut BufferCollection, conn: &mut Client) {

        /* All orders in the vector were filled by 1 new order,
         * so we have to handle 2 cases.
         *  1. Update all accounts who's orders were filled by new order.
         *  2. Update account of user who's order filled the old orders.
         **/

        // Map of {users: freshly executed trades}
        let mut update_map: HashMap<i32, Vec<Trade>> = HashMap::new();

        // Fill update_map
        for trade in trades.iter() {
            let entry = update_map.entry(trade.filled_uid).or_insert(Vec::with_capacity(trades.len()));
            entry.push(trade.clone());
        }

        // Case 1
        // TODO: This is a good candidate for multithreading.
        for (user_id, new_trades) in update_map.iter() {
            self.update_single_user(buffers, *user_id, new_trades, false, conn);
        }
        // Case 2: update account who placed order that filled others.
        self.update_single_user(buffers, trades[0].filler_uid, &trades, true, conn);

        // TODO: PER-5
        //  Instead of writing this trade insert here, we should be writing to a buffer of trades
        //  which will be emptied occasionally when full.
        //
        // For each trade, insert into ExecutedTrades table.
        database::write_insert_trades(trades, conn);
        buffers.buffered_trades.add_trades_to_buffer(trades); // PER-5 update
    }

    pub fn print_all(&self) {
        println!("PRINT_ALL UNDER CONSUTRUCTION DURING DB MIGRATION");
        /*
        for (k, v) in self.users.iter() {
            match self.authenticate(&k, &v.password) {
                Ok(_) => self.print_user(&k, true),
                Err(_) => ()
            }
        }
        */
    }
}
