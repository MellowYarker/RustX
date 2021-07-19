use crate::exchange::requests::{Order, OrderStatus};
use crate::exchange::filled::Trade;

use std::collections::HashMap;

use postgres::Client;
use crate::database;
use chrono::{DateTime, FixedOffset};

use redis::{Commands, RedisError};

use crate::buffer::BufferCollection;

// Error types for authentication
pub enum AuthError<'a> {
    NoUser(&'a String), // Username
    BadPassword(Option<String>), // optional error msg
}

/* This struct stores the pending orders of an account,
 * provides methods to access/update a pending order,
 * and can inform us if we're storing all known orders
 * on the exchange.
 *
 * Quick architecture note about pending.
 * Format: { "symbol" => {"order_id" => Order} }
 * This means we can find pending orders by first
 * looking up the symbol, then the order ID.
 *  - Solves 2 problems at once
 *      1. Very easy to check if a pending order has been filled.
 *      2. Fast access to orders in each market (see validate_order function).
 *
 **/
#[derive(Debug, Clone)]
pub struct AccountPendingOrders {
    pub pending: HashMap<String, HashMap<i32, Order>>,   // Orders that have not been completely filled.
    pub is_complete: bool // a simple bool that represents if this account has a full picture of their orders.
}

impl AccountPendingOrders {
    pub fn new() -> Self {
        let pending: HashMap<String, HashMap<i32, Order>> = HashMap::new();
        AccountPendingOrders {
            pending,
            is_complete: false
        }
    }

    /* Returns a mutable reference to a market of pending orders in an account. */
    pub fn get_mut_market(&mut self, symbol: &str) -> &mut HashMap<i32, Order> {
        self.pending.entry(symbol.clone().to_string()).or_insert(HashMap::new())
    }

    /* Insert an order into an accounts pending orders. */
    pub fn insert_order(&mut self, order: Order) {
        let market = self.get_mut_market(&order.symbol.as_str());
        market.insert(order.order_id, order);
    }

    /* After we've fetched this accounts pending orders, we call this to update the
     * state of the account. This just lets the program know we don't need to fetch
     * the orders again until the account is evicted from the cache.
     **/
    pub fn update_after_fetch(&mut self) {
        self.is_complete = true;
    }

    pub fn view_market(&self, symbol: &str) -> Option<&HashMap<i32, Order>> {
        self.pending.get(symbol)
    }

    /* Gives an immutable reference to an accounts pending order in a specific market. */
    pub fn get_order_in_market(&self, symbol: &str, id: i32) -> Option<&Order> {
        if let Some(market) = self.view_market(symbol) {
            return market.get(&id);
        }

        return None;
    }

    /* Removes a pending order from the Account.
     * The order IS in a market.
     **/
    pub fn remove_order(&mut self, symbol: &str, id: i32) {
        let market = self.get_mut_market(symbol);
        market.remove(&id);
    }
}

// Stores data about a user
#[derive(Debug, Clone)]
pub struct UserAccount {
    pub username: String,
    pub password: String,
    pub id: Option<i32>,
    pub pending_orders: AccountPendingOrders,
    pub recent_trades: Vec<Trade>, // Trades that occured since the user was brought into cache

    // recent_markets is: Market symbol followed by change in number of orders since user was cached.
    // If 2 orders were filled, and one new order was placed and is still pending (same market), the overall diff
    // is -1.
    pub recent_markets: HashMap<String, i32>,
    pub modified: bool  // bool representing whether account has been modified since last batch write to DB
}

impl UserAccount {
    pub fn from(username: &String, password: &String) -> Self {
        UserAccount {
            username: username.clone(),
            password: password.clone(),
            id: None, // We set this later
            pending_orders: AccountPendingOrders::new(),
            recent_trades: Vec::new(),
            recent_markets: HashMap::new(),
            modified: false,
        }
    }

    /* Used when reading values from database.*/
    pub fn direct(id: i32, username: &str, password: &str) -> Self {
        UserAccount {
            username: username.to_string().clone(),
             password: password.to_string().clone(),
            id: Some(id),
            pending_orders: AccountPendingOrders::new(),
            recent_trades: Vec::new(),
            recent_markets: HashMap::new(),
            modified: false,
        }
    }

    // Sets this account's user id, and returns it.
    fn set_id(&mut self, users: &Users) -> i32 {
        self.id = Some(users.total + 1);
        return self.id.unwrap();
    }

    /*
     * Returns None if this order *CANNOT* fill any pending orders placed by
     * this user. Otherwise, returns Some(Order) where Order is the pending order
     * that would be filled.
     *
     * Consider the following scenario:
     *  -   user places buy order for 10 shares of X at $10/share.
     *          - the order remains on the market and is not filled.
     *  -   later, the same user places a sell order for 3 shares of X at n <= $10/share.
     *  -   if there are no higher buy orders, the sell will fill the original buy.
     *  -   That is, the user will fill their own order (Trade with themselves).
     *
     *  We *could* check for this as we make trades, but I think it's better to make the user
     *  explicitly resubmit their order at a valid price.
     *
     * Note that this function can prevent an order from being placed, even if at the moment it was
     * placed, other pending orders were present that would prevent the new order from filling an
     * old one. This is because the program may be multi-threaded at some point, and so we cannot
     * be sure of order execution in extremely small time frames. I'm effectively preventing future
     * bugs.
     *
     **/
    pub fn validate_order(&self, order: &Order) -> Option<Order> {
        if !self.pending_orders.is_complete {
            panic!("\
Well, you've done it again.
You called validate_order on an account with in-complete pending order data.");
        }

        match self.pending_orders.view_market(&order.symbol.as_str()) {
            // We only care about the market that `order` is being submitted to.
            Some(market) => {
                let candidates = market.values().filter(|candidate| order.action.ne(&candidate.action));
                match order.action.as_str() {
                    "BUY" => {
                        let result = candidates.min_by(|x, y| x.price.partial_cmp(&y.price).expect("Tried to compare NaN!"));
                        if let Some(lowest_offer) = result {
                            if lowest_offer.price <= order.price {
                                return Some(lowest_offer.clone());
                            }
                        }
                    },
                    "SELL" => {
                        let result = candidates.max_by(|x, y| x.price.partial_cmp(&y.price).expect("Tried to compare Nan!"));
                        if let Some(highest_bid) = result {
                            if order.price <= highest_bid.price {
                                return Some(highest_bid.clone());
                            }
                        }
                    },
                    _ => ()
                }
            },
            None => ()
        }
        return None;
    }

    /* If the order is in the cache, we return its action (buy/sell), else None. */
    fn check_pending_order_cache(&self, symbol: &String, id: i32) -> Option<String> {
        if !self.pending_orders.is_complete {
            panic!("Tried to check pending order cache but the account does not have up to date pending orders.");
        }

        if let Some(order) = self.pending_orders.get_order_in_market(symbol.as_str(), id) {
            return Some(order.action.clone()); // buy or sell
        }

        return None;
    }

    /* Check if the user with the given username owns a pending order with this id.
     * If they do, return the order's action.
     **/
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
        self.pending_orders.remove_order(symbol.as_str(), id);
    }

    /* Prints the account information of this user
     * if their account view is up to date.
     **/
    pub fn print_user(&self) {
        if !self.pending_orders.is_complete {
            panic!("Tried to print_user who doesn't have complete pending order info!");
        }

        println!("\nAccount information for user: {}", self.username);

        if !self.pending_orders.pending.is_empty() {
            println!("\n\tOrders Awaiting Execution");
            for (_, market) in self.pending_orders.pending.iter() {
                for (_, order) in market.iter() {
                    println!("\t\t{:?}", order);
                }
            }
        } else {
            println!("\n\tNo Orders awaiting Execution");
        }

        // TODO: Make a separate method/function that populates executed_trades, this is too many
        // lines for this function IMO.
        let client = redis::Client::open("redis://127.0.0.1/").expect("Failed to open redis");
        let mut redis_conn = client.get_connection().expect("Failed to connect to redis");

        let mut executed_trades: Vec<Trade> = Vec::new();

        let filler_list = format!["filler:{}", self.id.unwrap()];
        let filled_list = format!["filled:{}", self.id.unwrap()];
        let lists = vec![filler_list, filled_list];
        for list in lists {
            let response: Result<Vec<String>, RedisError> = redis_conn.lrange(list, 0, -1);
            match response {
                Ok(trades) => {
                    for trade in trades {
                        let mut components = trade.split_whitespace();
                        let symbol: &str          = components.next().unwrap();
                        let action: &str          = components.next().unwrap();
                        let price: f64              = components.next().unwrap().to_string().trim().parse::<f64>().unwrap();
                        let filled_oid: i32         = components.next().unwrap().to_string().trim().parse::<i32>().unwrap();
                        let filled_uid: i32         = components.next().unwrap().to_string().trim().parse::<i32>().unwrap();
                        let filler_oid: i32         = components.next().unwrap().to_string().trim().parse::<i32>().unwrap();
                        let filler_uid: i32         = components.next().unwrap().to_string().trim().parse::<i32>().unwrap();
                        let exchanged: i32          = components.next().unwrap().to_string().trim().parse::<i32>().unwrap();

                        let mut naive_time = components.next().unwrap().to_string().replace("_", "T");
                        naive_time.push_str("+00:00");

                        let execution_time:
                            DateTime<FixedOffset>   = DateTime::parse_from_rfc3339(&naive_time.as_str()).unwrap();

                        executed_trades.push(Trade::direct(symbol,
                                                           action,
                                                           price,
                                                           filled_oid,
                                                           filled_uid,
                                                           filler_oid,
                                                           filler_uid,
                                                           exchanged,
                                                           execution_time)
                                             );
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                }
            }

        }
        // Get any trades that have occured since the user was cached.
        if self.recent_trades.len() > 0 {
            executed_trades.append(&mut (self.recent_trades.clone()));
        }

        if executed_trades.len() > 0 {
            println!("\n\tExecuted Trades");
            for order in executed_trades.iter() {
                println!("\t\t{:?}", order);
            }
        } else {
            println!("\n\tNo Executed Trades to show");
        }
        println!("\n");
    }


    /* Update the redis cache active_markets:user_id.
     * If we decrement a market to 0, then we remove it from the sorted set.
     **/
    fn redis_update_active_markets(&self, redis_conn: &mut redis::Connection) {
        for (market, diff) in self.recent_markets.iter() {

            let mut delete_required = false;
            let response: Result<String, RedisError> = redis_conn.zincr(format!["active_markets:{}", self.id.unwrap()], market, *diff);

            match response {
                Ok(count) => {
                    let count = count.trim().parse::<i32>().unwrap();
                    if count == 0 {
                        // Remove the value from the set.
                        delete_required = true;
                    } else if count < 0 {
                        eprintln!("There is a bug in active_markets:{}. There are {} pending orders in our redis cache.", self.id.unwrap(), count);
                        panic!("This is a bug, the programmer needs to find the bad logic!");
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                }
            }

            if delete_required {
                let _: () = redis_conn.zrem(format!["active_markets:{}", self.id.unwrap()], market).unwrap();
            }
        }
    }

    /* Flush the user's recent trades to Redis.
     * We call this when users are evicted from cache,
     * including on program shutdown.
     *
     * TODO: Replace _ with T, append +00:00 to date, then remove these from deconstruction later.
     *
     * TODO: Make 2 iterators, one for filled, one for filler,
     *       then batch insert all trades into each list, rather
     *       than do 1 request per trade.
     **/
    fn flush_trades_to_redis(self, redis_conn: &mut redis::Connection) {

        let filler_trades = self.recent_trades.iter().cloned().filter(|trade| trade.filler_uid == self.id.unwrap());
        let filled_trades = self.recent_trades.iter().cloned().filter(|trade| trade.filled_uid == self.id.unwrap());

        // TODO: If we can figure out multiple item inserts, use these.
        // let mut filler_args: Vec<String> = Vec::new();
        // let mut filled_args: Vec<String> = Vec::new();

        for trade in filler_trades {
            let time: String = format!["{}", trade.execution_time];
            let mut components = time.split_whitespace();
            let time = format!["{}_{}", components.next().unwrap(), components.next().unwrap()];

            let args = format!["{} {} {} {} {} {} {} {} {}", trade.symbol, trade.action, trade.price, trade.filled_oid, trade.filled_uid, trade.filler_oid, trade.filler_uid, trade.exchanged, time];

            let filler_response: Result<i32, RedisError> = redis_conn.lpush(&format!["filler:{}", self.id.unwrap()], args);
            if let Err(e) =  filler_response {
                eprintln!("{}", e);
            }
            // filler_args.push(format!["{} {} {} {} {} {} {} {} {}", trade.symbol, trade.action, trade.price, trade.filled_oid, trade.filled_uid, trade.filler_oid, trade.filler_uid, trade.exchanged, time]);
        }

        for trade in filled_trades {
            let time: String = format!["{}", trade.execution_time];
            let mut components = time.split_whitespace();
            let time = format!["{}_{}", components.next().unwrap(), components.next().unwrap()];
            let args = format!["{} {} {} {} {} {} {} {} {}", trade.symbol, trade.action, trade.price, trade.filled_oid, trade.filled_uid, trade.filler_oid, trade.filler_uid, trade.exchanged, time];

            let filled_response: Result<i32, RedisError> = redis_conn.lpush(&format!["filled:{}", self.id.unwrap()], args);
            if let Err(e) = filled_response {
                eprintln!("{}", e);
            }
            // filled_args.push(format!["{} {} {} {} {} {} {} {} {}", trade.symbol, trade.action, trade.price, trade.filled_oid, trade.filled_uid, trade.filler_oid, trade.filler_uid, trade.exchanged, time]);
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
//          the user will pass their Username/Password combo to do things on our exchange.
//
//          Since orders (and eventually most things) are associated with users by user_ID,
//          and the users don't know their ID until we have a proper frontend that can memorize
//          that data, we'll need to change our data structures.
// ------------------------------------------------------------------------------------------------------
pub struct Users {
    users: HashMap<String, UserAccount>,
    // TODO: This should be an LRU cache eventually
    id_map: HashMap<i32, String>,   // maps user_id to username
    pub redis_conn: redis::Connection,
    total: i32,
}

impl Users {

    pub fn new() -> Self {
        // TODO: How do we want to decide what the max # users is?
        let max_users = 1000;
        let users: HashMap<String, UserAccount> = HashMap::with_capacity(max_users);
        let id_map: HashMap<i32, String> = HashMap::with_capacity(max_users);

        let client = redis::Client::open("redis://127.0.0.1/").expect("Failed to open redis");
        let redis_conn = client.get_connection().expect("Failed to connect to redis");

        Users {
            users,
            id_map,
            redis_conn,
            total: 0
        }
    }

    /* Update the total user count. */
    pub fn direct_update_total(&mut self, conn: &mut Client) {
        self.total = database::read_total_accounts(conn);
    }

    /* Set all UserAccount's modified field to false. */
    pub fn reset_users_modified(&mut self) {
        for (_key, entry) in self.users.iter_mut() {
            entry.modified = false;
        }
    }

    /* TODO: Some later PR, create a new thread to make new accounts.
     *
     * If an account with this username exists, do nothing, otherwise
     * add the account to the database and return it's ID.
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

    /* Stores a user in the programs cache.
     * If a user is successfully added to the cache, we return true, otherwise, return false.
     **/
    fn cache_user(&mut self, account: UserAccount) {
        // Evict a user if we don't have space.
        let capacity: f64 = self.users.capacity() as f64;
        let count: f64 = self.users.len() as f64;
        if capacity * 0.9 <= count {
            // If no one good eviction candidates, force evictions.
            if !self.evict_user(false) {
                self.evict_user(true);
            }
        }

        self.id_map.insert(account.id.unwrap(), account.username.clone());
        self.users.insert(account.username.clone(), account);
    }

    /* Evict a user from the cache.
     * If a user was evicted successfully, return true, else return false.
     *
     * We can only evict users who's modified fields are set to false.
     * This is the only constraint on our cache eviction policy.
     *
     * We can have extremely simple cache eviction, ex, random or
     * evict first candidate found.
     *
     * We could have extremely complicated cache eviction, ex.
     *      - keep a ranking of users by likelihood they will be
     *        modified again. Track things like:
     *          - likelihood of an order being filled (track all orders in all markets).
     *          - likelihood of *placing an order* again soon
     *          - likelihood of cancelling an order soon
     *
     *  It remains to be seen if a basic cache eviction policy is good enough.
     *
     * On cache eviction, write all recent_trades of the evicted user to Redis!
     **/
    fn evict_user(&mut self, force_evict: bool) -> bool {
        // POLICY: Delete first candidate
        //     Itereate over all the entries, once we find one that's not modified, stop
        //     iterating, make note of the key, then delete the entry.

        let mut key_to_evict: Option<i32> = None;

        for (_name, entry) in self.users.iter() {
            if (!entry.pending_orders.is_complete) || force_evict {
                key_to_evict = entry.id;
                break;
            }
        }

        // If we found a user we can evict
        if let Some(key) = key_to_evict {
            let username = self.id_map.remove(&key).unwrap(); // returns the value (username)
            let evicted = self.users.remove(&username).unwrap();

            // Write the cached data to redis
            evicted.redis_update_active_markets(&mut self.redis_conn);
            evicted.flush_trades_to_redis(&mut self.redis_conn);
            return true;
        }
        // Failed to evict a user.
        return false;
    }

    /* On shutdown, we flush all recent_trades and recent_markets to Redis. */
    pub fn flush_user_cache(&mut self) {
        for user in self.users.values().cloned() {
            user.redis_update_active_markets(&mut self.redis_conn);
            user.flush_trades_to_redis(&mut self.redis_conn);
        }
    }

    /* Check the redis cache for the user, on success we return Some(user),
     * on failure we return None.
     **/
    fn check_redis_user_cache(&mut self, username: &str) -> Result<Option<UserAccount>, RedisError> {
        let response: Result<HashMap<String, String>, RedisError> = self.redis_conn.hgetall(format!["user:{}", username]);
        match response {
            Ok(map) => {
                let id: i32;
                let mut password = String::new();

                if let Some(val) =  map.get("id") {
                    id = val.trim().parse::<i32>().unwrap();
                    password.push_str(map.get("password").unwrap());
                    return Ok(Some(UserAccount::direct(id, username, &password)));
                }
                return Ok(None);
            },
            // Otherwise we got an err
            Err(e) => return Err(e)
        }
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
    pub fn authenticate<'a>(&mut self, username: &'a String, password: &String, conn: &mut Client) -> Result<&mut UserAccount, AuthError<'a>> {
        // First, we check our in-memory cache
        let mut cache_miss = true;
        let mut redis_miss = true;
        match self.auth_check_cache(username, password) {
            Ok(()) => {
                cache_miss = false;
                redis_miss = false;
            }
            Err(e) => {
                if let AuthError::BadPassword(_) = e {
                    return Err(e);
                };
            }
        }

        // On cache miss, check redis.
        if cache_miss {
            match self.check_redis_user_cache(username.as_str()) {
                Ok(acc) => {
                    if let Some(account) = acc {
                        // TODO: I don't like that we read the password into the program.
                        // I would rather have it be checked in Redis like postgres does,
                        // since they may do security better. But then again, I've heard
                        // redis security isn't great.
                        if &account.password == password {
                            // Cache the user we found
                            self.cache_user(account.clone());
                            redis_miss = false;
                        } else {
                            return Err(AuthError::BadPassword(None));
                        }
                    }
                },
                Err(e) => {
                    eprintln!("{}", e);
                    panic!("Something went wrong with redis.");
                }
            }
        }
        // On redis cache miss, check the database.
        if redis_miss {
            match database::read_auth_user(username, password, conn) {
                // We got an account, move it into the cache.
                Ok(account) => {

                    // Copy of the id
                    let id = account.id.unwrap();

                    // If we fail to cache the user, flush the buffers so we can evict users.
                    self.cache_user(account.clone());

                    // Finally, cache the user in redis
                    let id = id.to_string();
                    let v = vec![   ("id", id.as_str()),
                                    ("username", username),
                                    ("password", password)];
                    let _: () = self.redis_conn.hset_multiple(format!["user:{}", username], &v[..]).unwrap();
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
        return Ok(self.users.get_mut(username).unwrap());
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

    /* For internal use only.
     *
     * If the account is in the cache (active user), we return a mutable ref to the user.
     * If the account is in the database, we construct a user, cache them, get the pending orders,
     * then return the UserAccount to the calling function.
     */
    fn _get_mut(&mut self, username: &String, conn: &mut Client) -> &mut UserAccount {
        match self.users.get_mut(username) {
            Some(_) => (),
            None => {
                // TODO: First check redis, then check DB if redis fails.
                let account: UserAccount;
                let redis_response = match self.check_redis_user_cache(username.as_str()) {
                    Ok(response) => response,
                    Err(e) => {
                        eprintln!("{}", e);
                        panic!("Something went wrong while trying to get a user from Redis!")
                    }
                };
                // If we didn't find the user in Redis, check DB.
                if let None = redis_response {
                    account = match database::read_account(username, conn) {
                        Ok(acc) => acc,
                        Err(_) => panic!("Something went wrong while trying to get a user from the database!")
                    };
                } else {
                    account = redis_response.unwrap();
                }

                self.cache_user(account.clone());
            }
        }
        return self.users.get_mut(username).unwrap();
    }

    /* Returns a username if one is found. */
    fn redis_get_id_map(&mut self, id: i32) -> Option<String> {
        let response: Result<Option<String>, RedisError> = self.redis_conn.hget(format!["id:{}", id], "username");

        if let Ok(potential_name) = response {
            if let Some(name) = potential_name {
                return Some(String::from(name));
            }
        }

        return None;
    }

    /* Update this users pending_orders, and the Orders table.
     * We have 2 cases to consider, as explained in update_account_orders().
     **/
    fn update_single_user(&mut self, buffers: &mut BufferCollection, id: i32, modified_orders: &Vec<Order>, trades: &Vec<Trade>, is_filler: bool, conn: &mut Client) {
        // TODO:
        //  At some point, we want to get the username by calling some helper access function.
        //  This new function will
        //      1. Check the id_map cache
        //      2. If ID not found, check the database
        //      3. Update the id_map cache (LRU)

        let username: String = match self.id_map.get(&id) {
            Some(name) => name.clone(),
            None => {
                // Check redis for the user id -> username map
                let response = self.redis_get_id_map(id);
                // wasn't in redis, check the database.
                if let None = response {
                    let result = database::read_user_by_id(id, conn);
                    if let Err(_) = result {
                        panic!("Query to get user by id failed!");
                    };

                    // Store this in redis now.
                    let _: () = self.redis_conn.hset(format!["id:{}", id], "username", result.as_ref().unwrap()).unwrap();
                    result.unwrap()
                } else {
                    // name found in redis
                    response.unwrap()
                }
            }
        };

        // Gives a mutable reference to cache.
        let account = self._get_mut(&username, conn);

        // PER-6 set account modified to true because we're modifying their orders.
        account.modified = true;

        // Since we can't remove entries while iterating, store the key's here.
        // We know we won't need more than trade.len() entries.
        let mut entries_to_remove: Vec<i32> = Vec::with_capacity(trades.len());

        // constant strings
        const BUY: &str = "BUY";
        const SELL: &str = "SELL";

        let account_market = account.pending_orders.get_mut_market(&trades[0].symbol.as_str());

        // Iterate over the trades, storing them + modifying orders in the users
        // respective accounts and the buffers.
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

                // Since this account is the filler, we know every trade belongs to them
                account.recent_trades.push(update_trade);
            } else {
                // If this user placed the order that was filled,
                // add the trade to their account.
                if update_trade.filled_uid == account.id.unwrap() {
                    account.recent_trades.push(update_trade);
                }
            }

            // After processing the order, move it to executed trades.
            match account_market.get_mut(&id) {
                Some(order) => {
                    // We don't want to modify the filler's order at all, as that is
                    // done earlier (when we first submitted it to the market).
                    if !is_filler && (trade.exchanged == (order.quantity - order.filled)) {
                        // Add/update this completed order in the database buffer.
                        order.status = OrderStatus::COMPLETE;
                        order.filled = order.quantity;
                        buffers.buffered_orders.add_or_update_entry_in_order_buffer(&order, true); // PER-5 update

                        entries_to_remove.push(order.order_id);
                        // Get the entry in the recent_markets map, we want to decrement it by 1.
                        let market_diff = account.recent_markets.entry(order.symbol.clone()).or_insert(0);
                        *market_diff -= 1;
                    } else if !is_filler {
                        // Don't update the filler's filled count,
                        // new orders are added to accounts in submit_order_to_market.
                        order.filled += trade.exchanged;

                        // Add/update this pre-existing pending order to the database buffer.
                        buffers.buffered_orders.add_or_update_entry_in_order_buffer(&order, true); // PER-5 update
                    }
                },
                // Order not found in users in-mem account, this is because
                // the user hasn't placed/cancelled an order recently.
                // This is fine, as we can read the order from the modified_orders vector.
                None => {
                    for order in modified_orders.iter() {
                        if order.order_id == id {
                            let market_diff = account.recent_markets.entry(order.symbol.clone()).or_insert(0);
                            if let OrderStatus::PENDING = order.status {
                                account_market.insert(id, order.clone());
                            } else {
                                *market_diff -= 1;
                            }
                            buffers.buffered_orders.add_or_update_entry_in_order_buffer(&order, true);
                            break;
                        }
                    }
                }
            }
        }

        // Remove any completed orders from the accounts pending orders.
        for i in &entries_to_remove {
            account_market.remove(i);
        }
    }

    /* Given a vector of Trades, update all the accounts
     * that had orders filled.
     */
    pub fn update_account_orders(&mut self, modified_orders: &mut Vec<Order>, trades: &mut Vec<Trade>, buffers: &mut BufferCollection, conn: &mut Client) {

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
            self.update_single_user(buffers, *user_id, modified_orders, new_trades, false, conn);
        }
        // Case 2: update account who placed order that filled others.
        self.update_single_user(buffers, trades[0].filler_uid, modified_orders, trades, true, conn);

        // Add this trade to the trades database buffer.
        buffers.buffered_trades.add_trades_to_buffer(trades); // PER-5 update
    }
}
