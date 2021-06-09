use crate::exchange::requests::Order;
use crate::exchange::filled::Trade;

use std::collections::HashMap;
use std::convert::TryFrom;

use postgres::{Client, NoTls};
use crate::database;

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
    // pub executed_trades: Vec<Trade>                             // Trades that have occurred.
}

impl UserAccount {
    pub fn from(username: &String, password: &String) -> Self {
        let placed: HashMap<String, HashMap<i32, Order>> = HashMap::new();
        // let trades: Vec<Trade> = Vec::new();
        UserAccount {
            username: username.clone(),
            password: password.clone(),
            id: None, // We set this later
            pending_orders: placed,
            // executed_trades: trades,
        }
    }

    /* Used when reading values from database.
     * */
    pub fn direct(id: i32, username: &str, password: &str) -> Self {
        let placed: HashMap<String, HashMap<i32, Order>> = HashMap::new();
        // let trades: Vec<Trade> = Vec::new();
        UserAccount {
            username: username.to_string().clone(),
            password: password.to_string().clone(),
            id: Some(id),
            pending_orders: placed,
            // executed_trades: trades,
        }
    }

    // Sets this account's user id, and returns it.
    fn set_id(&mut self, users: &Users) -> i32 {
        self.id = Some(users.total + 1);
        return self.id.unwrap();
    }

    /* TODO: Order inserts by time executed!
     * Get this accounts pending orders from the database.
     **/
    fn fetch_account_pending_orders(&mut self, conn: &mut Client) {
        let query_string = "\
SELECT o.* FROM Orders o, PendingOrders P
WHERE o.order_ID = p.order_ID
AND o.user_ID =
    (SELECT ID FROM Account
     WHERE Account.username = $1)
ORDER BY o.order_ID;";
        for row in conn.query(query_string, &[&self.username]).expect("Query to fetch pending orders failed!") {
            let order_id:       i32  = row.get(0);
            let symbol:         &str = row.get(1);
            let action:         &str = row.get(2);
            let quantity:       i32  = row.get(3);
            let filled:         i32  = row.get(4);
            let price:          f64  = row.get(5);
            let user_id:        i32  = row.get(6);
            // let status:         i32  = row.get(7); // <---- TODO
            // let time_placed:    i32  = row.get(7); // <---- TODO
            // let time_updated:   i32  = row.get(7); // <---- TODO

            // We will just re-insert everything.
            let order = Order::direct(action,
                                      symbol,
                                      quantity,
                                      filled,
                                      price,
                                      order_id,
                                      user_id);
            let market = self.pending_orders.entry(order.symbol.clone()).or_insert(HashMap::new());
            market.insert(order.order_id, order);
        }
    }

    /* TODO: Order inserts by time executed!
     * Get this accounts executed trades from the database.
     **/
    fn fetch_account_executed_trades(&self, executed_trades: &mut Vec<Trade>, conn: &mut Client) {
        // let mut executed_trades: Vec<Trade> = Vec::new();
        // self.executed_trades.clear();
        // First, lets get trades where we had our order filled.
        let query_string = "\
SELECT * FROM ExecutedTrades e
WHERE e.filled_UID =
    (SELECT ID FROM Account WHERE Account.username = $1)
ORDER BY e.filled_OID;";

        for row in conn.query(query_string, &[&self.username]).expect("Query to fetch executed trades failed!") {
            let symbol:     &str = row.get(0);
            let action:     &str = row.get(1);
            let price:      f64  = row.get(2);
            let filled_oid: i32  = row.get(3);
            let filled_uid: i32  = row.get(4);
            let filler_oid: i32  = row.get(5);
            let filler_uid: i32  = row.get(6);
            let exchanged:  i32  = row.get(7);
            // let exec_time:  date  = row.get(8); // <--- TODO
            let trade = Trade::direct(symbol,
                                      action,
                                      price,
                                      filled_oid,
                                      filled_uid,
                                      filler_oid,
                                      filler_uid,
                                      exchanged);
            executed_trades.push(trade);
        }

        // Next, lets get trades where we were the filler.
        let query_string = "\
SELECT * FROM ExecutedTrades e
WHERE e.filler_UID =
    (SELECT ID FROM Account WHERE Account.username = $1)
ORDER BY e.filled_OID;";

        for row in conn.query(query_string, &[&self.username]).expect("Query to fetch executed trades failed!") {
            let symbol:     &str = row.get(0);
            let mut action: &str = row.get(1);
            let price:      f64  = row.get(2);
            let filled_oid: i32  = row.get(3);
            let filled_uid: i32  = row.get(4);
            let filler_oid: i32  = row.get(5);
            let filler_uid: i32  = row.get(6);
            let exchanged:  i32  = row.get(7);
            // let exec_time:  date  = row.get(8); // <--- TODO

            // Switch the action because we were the filler.
            match action.to_string().as_str() {
                "BUY" => action = "SELL",
                "SELL" => action = "BUY",
                _ => ()
            }

            let trade = Trade::direct(symbol,
                                      action,
                                      price,
                                      filled_oid,
                                      filled_uid,
                                      filler_oid,
                                      filler_uid,
                                      exchanged);
            executed_trades.push(trade);
        }
    }

    /*
     * Consider the following scenario:
     *  -   user places buy order for 10 shares of X at $10/share.
     *          - the order remains on the market and is not filled.
     *  -   later, the same user places a sell order for 3 shares of X at n <= $10/share.
     *  -   the new order will fill their old order, which is probably undesirable,
     *      or even illegal.
     *
     *  Returns true if this order will not fill any pending orders placed by
     *  this user. Otherwise, returns false.
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

    fn check_order_cache(&self, symbol: &String, id: i32) -> Option<String> {
        if let Some(market) = self.pending_orders.get(symbol) {
            if let Some(order) = market.get(&id) {
                return Some(order.action.clone()); // buy or sell
            }
        }

        return None;
    }

    /* Check if the user with the given username actually placed this order. */
    pub fn user_placed_order(&self, symbol: &String, id: i32) -> Option<String> {
        match self.check_order_cache(symbol, id) {
            Some(action) => return Some(action),
            None => {
                // Update cache and try again!
                // self.fetch_account_pending_orders(
            }
        }
        /* TODO:
        // Check the in program cache, if not there, check db?
        if let Some(market) = self.pending_orders.get(symbol) {
            if let Some(order) = market.get(&id) {
                return Some(order.action.clone()); // buy or sell
            }
        }
        */

        return None;
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

    /* TODO: Find a better way to do this.
     * Insert a user to program cache from database
     */
    pub fn populate_from_db(&mut self, conn: &mut Client) {
        for row in conn.query("SELECT id, username, password FROM Account", &[]).expect("Something went wrong in the query.") {
            let id: i32 = row.get(0);
            let username: &str = row.get(1);
            let password: &str = row.get(2);

            let account = UserAccount::direct(id, username, password);
            self.cache_user(account);

            let authenticated = true;
            if let Ok(account) = self.get_mut(&username.to_string(), authenticated) {
                account.fetch_account_pending_orders(conn);
            }
        }
    }

    /* TODO: This is horrible. We need to move populate_from_db and this function
     * into the database.rs file, and group them into 1 function.
     * */
    pub fn direct_update_total(&mut self, conn: &mut Client) {
        for row in conn.query("SELECT count(*) FROM Account", &[]).expect("Something went wrong in the query.") {
            let count: i64 = row.get(0);
            self.total = i32::try_from(count).unwrap();
        }
    }

    /* If an account with this username exists, do nothing, otherwise
     * add the account and return it's ID.
     */
    pub fn new_account(&mut self, account: UserAccount, conn: &mut Client) -> Option<i32> {
        // User is cached already
        if self.users.contains_key(&account.username) {
            return None;
        } else {
            let query_string = "SELECT ID FROM Account WHERE Account.username=$1";
            for row in conn.query(query_string, &[&account.username]) {
                // If a user exists, return None
                if let Some(_) = row.get(0) {
                    return None;
                }
            }
            // User doesn't exist, so create a new one.
            let mut account = account;
            self.total = account.set_id(&self);

            // Insert to db
            // TODO: Insert regsiter_time.
            let query_string = "INSERT INTO Account (ID, username, password) VALUES ($1, $2, $3);";
            match conn.execute(query_string, &[&account.id.unwrap(), &account.username, &account.password]) {
                Ok(_) => {
                    // Cache in program
                    self.cache_user(account);
                    return Some(self.total);
                },
                Err(e) => {
                    eprintln!("{:?}", e);
                    eprintln!("Something went wrong with the insert!");
                }
            }
            // TODO:
            //  This should actually return an Error because we failed to insert to the database!
            return None;
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

    /* Checks the database for this user.*/
    fn auth_check_db<'a>(&self, username: &'a String, password: & String, conn: &mut Client) -> Result<UserAccount, AuthError<'a>> {
        let query_string = "SELECT ID, username, password FROM Account WHERE Account.username = $1";
        let result = conn.query(query_string, &[&username]).expect("Something went wrong with the authenticate query.");

        // Did not find the user
        if result.len() == 0 {
            return Err(AuthError::NoUser(username));
        }

        // Found a user, usernames are unique so we get 1 row.
        let row = &result[0];
        let recv_id: i32 = row.get(0);
        let recv_username: &str = row.get(1);
        let recv_password: &str = row.get(2);

        // User authenticated.
        if *password == recv_password {
            return Ok(UserAccount::direct(recv_id, recv_username, recv_password));
        }

        // Password was incorrect.
        return Err(AuthError::BadPassword(None));
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
            match self.auth_check_db(username, password, conn) {
                // We got an account, move it into the cache.
                Ok(mut account) => {
                    // Since our user will be cached, and we are likely to do things with the user.
                    // We should probably make sure the in-mem pending orders are consistent w/ the database!
                    account.fetch_account_pending_orders(conn);
                    self.cache_user(account);
                },
                Err(e) => return Err(e)
            }
        }

        // TODO: we call get twice if it was a cache hit.
        //       This is clearly stupid, but Rust's borrow checker is mad at me again,
        //       so I will figure this out later.
        return Ok(self.users.get(username).unwrap());
    }

    /* Returns a reference to a user account if
     * user has been authenticated.
     */
    pub fn get<'a>(&mut self, username: &'a String, authenticated: bool) -> Result<&UserAccount, AuthError<'a>> {
        if authenticated {
            match self.users.get(username) {
                // Cached
                Some(_) => (),//return Ok(account),
                // In database
                None => {
                    let mut client = Client::connect("host=localhost user=postgres dbname=mydb", NoTls).expect("Failed to connect to Database. Please ensure it is up and running.");
                    let result = client.query("SELECT ID, username, password FROM Account where Account.username = $1", &[username]).expect("Failed to get user from database.");

                    let row = &result[0];
                    let recv_id: i32 = row.get(0);
                    let recv_username: &str = row.get(1);
                    let recv_password: &str = row.get(2);
                    // TODO: Do we want to cache the user?
                    let account = UserAccount::direct(recv_id, recv_username, recv_password);
                    self.cache_user(account);
                }
            }
            // TODO: we call get twice if it was a cache hit.
            //       This is clearly stupid, but Rust's borrow checker is mad at me again,
            //       so I will figure this out later.
            return Ok(self.users.get(username).unwrap());
        }
        let err_msg = format!["Must authenticate before accessing account belonging to: ({})", username];
        return Err(AuthError::BadPassword(Some(err_msg)));
    }

    /* TODO: Return a Result<T, E> instead of Option so we
     *       can specify any errors!
     * Returns a mutable reference to a user account if:
     *  - the account exists and
     *  - the user has been authenticated
     */
    pub fn get_mut<'a>(&mut self, username: &'a String, authenticated: bool) -> Result<&mut UserAccount, AuthError<'a>> {
        if authenticated {
            match self.users.get_mut(username) {
                // Cached
                Some(_) => (),//return Ok(account),
                // In database
                None => {
                    let mut client = Client::connect("host=localhost user=postgres dbname=mydb", NoTls).expect("Failed to connect to Database. Please ensure it is up and running.");
                    let result = client.query("SELECT ID, username, password FROM Account where Account.username = $1", &[username]).expect("Failed to get user from database.");

                    let row = &result[0];
                    let recv_id: i32 = row.get(0);
                    let recv_username: &str = row.get(1);
                    let recv_password: &str = row.get(2);
                    // TODO: Do we want to cache the user?
                    let account = UserAccount::direct(recv_id, recv_username, recv_password);
                    self.cache_user(account);
                }
            }
            // TODO: we call get twice if it was a cache hit.
            //       This is clearly stupid, but Rust's borrow checker is mad at me again,
            //       so I will figure this out later.
            return Ok(self.users.get_mut(username).unwrap());
        }
        let err_msg = format!["Must authenticate before accessing account belonging to: ({})", username];
        return Err(AuthError::BadPassword(Some(err_msg)));
    }

    /* For internal use only.
     * Returns a mutable reference to a user account if:
     *  - the account exists
     * TODO: Is it stupid to have an entire function for this?
     *       The benefit is we indicate only internal functions access
     *       the `users` property.
     */
    fn _get_mut(&mut self, username: &String) -> Option<&mut UserAccount> {
        self.users.get_mut(username)
    }

    /* Prints the account information of this user if:
     *  - the account exists and
     *  - the password is correct for this user
     */
    pub fn print_user(&mut self, username: &String, authenticated: bool) {
        match self.get_mut(username, authenticated) {
            Ok(account) => {
                println!("\nAccount information for user: {}", account.username);

                let mut client = Client::connect("host=localhost user=postgres dbname=mydb", NoTls)
                    .expect("Failed to connect to Database. Please ensure it is up and running.");

                account.fetch_account_pending_orders(&mut client);
                let mut executed_trades: Vec<Trade> = Vec::new();
                account.fetch_account_executed_trades(&mut executed_trades, &mut client);

                println!("\n\tOrders Awaiting Execution");
                for (_, market) in account.pending_orders.iter() {
                    for (_, order) in market.iter() {
                        println!("\t\t{:?}", order);
                    }
                }
                println!("\n\tExecuted Trades");
                // for order in account.executed_trades.iter() {
                for order in executed_trades.iter() {
                    println!("\t\t{:?}", order);
                }
                println!("\n");
            },
            Err(e) => Users::print_auth_error(e)
        }
    }

    /* Update this users pending_orders and executed_trades.
     * We have 2 cases to consider, as explained in update_account_orders().
     **/
    fn update_single_user(&mut self, id: i32, trades: &Vec<Trade>, is_filler: bool, conn: &mut Client) {
        // TODO:
        //  At some point, we want to get the username by calling some helper access function.
        //  This new function will
        //      1. Check the id_map cache
        //      2. If ID not found, check the database
        //      3. Update the id_map cache (LRU)
        let username: String = self.id_map.get(&id).expect("update_single_user Error couldn't get username from userID").clone();
        let account = self._get_mut(&username).expect("update_single_user ERROR: couldn't find user!");
        // Since we can't remove entries while iterating, store the key's here.
        // We know we won't need more than trade.len() entries.
        let mut entries_to_remove: Vec<i32> = Vec::with_capacity(trades.len());

        // constant strings
        const BUY: &str = "BUY";
        const SELL: &str = "SELL";

        let market = account.pending_orders.entry(trades[0].symbol.clone()).or_insert(HashMap::new());

        // Query strings that we will extend.
        let mut update_filled_query_string = String::new();

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
                    if trade.exchanged == (order.quantity - order.filled) {
                        entries_to_remove.push(order.order_id); // order completely filled
                    } else if !is_filler {
                        // Don't update the filler's filled count,
                        // new orders are added to accounts in submit_order_to_market.
                        order.filled += trade.exchanged; // order partially filled
                        // Extend our query string
                        update_filled_query_string
                            .push_str(format!["UPDATE Orders set filled={} WHERE order_id={}; ", order.filled, order.order_id].as_str());
                    }
                    // account.executed_trades.push(update_trade);
                },
                // None => account.executed_trades.push(update_trade)
                None => ()
            }
            // account.executed_trades.push(update_trade);
        }

        // For each trade, update `filled` in Orders table.
        database::update_filled_counts(&update_filled_query_string, conn);

        // Remove any completed orders from the accounts pending orders.
        for i in &entries_to_remove {
            market.remove(&i);
        }
        // Remove all the completed orders from the database's pending table.
        // Sets Orders to complete, and sets filled = quantity.
        database::delete_pending_orders(&entries_to_remove, conn);
    }

    /* Given a vector of Trades, update all the accounts
     * that had orders filled.
     */
    pub fn update_account_orders(&mut self, trades: &Vec<Trade>) {

        /* All orders in the vector were filled by 1 new order,
         * so we have to handle 2 cases.
         *  1. Update all accounts who's orders were filled by new order.
         *  2. Update account of user who's order filled the old orders.
         **/

        let mut conn = Client::connect("host=localhost user=postgres dbname=mydb", NoTls)
            .expect("Failed to connect to Database. Please ensure it is up and running.");

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
            self.update_single_user(*user_id, new_trades, false, &mut conn);
        }
        // Case 2: update account who placed order that filled others.
        self.update_single_user(trades[0].filler_uid, &trades, true, &mut conn);

        // For each trade, insert into ExecutedTrades table.
        database::write_insert_trades(&trades, &mut conn);
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
