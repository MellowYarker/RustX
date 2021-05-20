use crate::exchange::requests::Order;
use crate::exchange::filled::FilledOrder;
use std::collections::HashMap;

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
    pub pending_orders: HashMap<i32, Order>,// Orders that have not been completely filled.
    pub executed_trades: Vec<FilledOrder>   // Trades that have occurred.
}

// Where we store all our users
pub struct Users {
    users: HashMap<String, UserAccount>,
    total: i32
}

impl UserAccount {
    pub fn from(name: &String, password: &String) -> Self {
        let placed: HashMap<i32, Order> = HashMap::new();
        let trades: Vec<FilledOrder> = Vec::new();
        UserAccount {
            username: name.to_string().clone(),
            password: password.to_string().clone(),
            id: None, // We set this later
            pending_orders: placed,
            executed_trades: trades,
        }
    }

    // Sets this account's user id, and returns it.
    pub fn set_id(&mut self, users: &Users) -> i32 {
        self.id = Some(users.total + 1);
        return self.id.unwrap();
    }
}

impl Users {

    pub fn print_auth_error(err: AuthError) {
        match err {
            AuthError::NoUser(user) => println!("Authentication failed! User ({}) not found.", user),
            AuthError::BadPassword(message) => if let Some(msg) = message {
                println!("{}", msg);
            } else {
                println!("Authentication failed! Incorrect password!")
            }
        }
    }
    /* If the username exists and the password is correct,
     * we do not return an error.
     *
     * If the user doesn't exist, or the user exists and the
     * password is incorrect, we return an error.
     *
     * TODO: Maybe we can return some type of session token
     *       for the frontend to hold on to?
     */
    pub fn authenticate<'a>(&self, username: &'a String, password: & String) -> Result<(), AuthError<'a>> {
        match self.users.get(username) {
            Some(account) => {
                if *password == account.password {
                    return Ok(());
                }
                return Err(AuthError::BadPassword(None))
            },
            None => ()
        }
        return Err(AuthError::NoUser(username));
    }

    pub fn new() -> Self {
        let map: HashMap<String, UserAccount> = HashMap::new();
        Users {
            users: map,
            total: 0
        }
    }

    /* If an account with this username exists, do nothing, otherwise
     * add the account and return it's ID.
     */
    pub fn new_account(&mut self, account: UserAccount) -> Option<i32> {
        if self.users.contains_key(&account.username) {
            return None;
        } else {
            let mut account = account;
            self.total = account.set_id(&self);
            self.users.insert(account.username.clone(), account);
            return Some(self.total);
        }
    }

    /* Returns a reference to a user account if:
     *  - the account exists and
     *  - the password is correct for this user
     */
    fn get<'a>(&self, username: &'a String, password: &String) -> Result<&UserAccount, AuthError<'a>> {
        match self.users.get(username) {
            Some(account) => {
                if *password == account.password {
                    return Ok(account);
                }
                return Err(AuthError::BadPassword(None));
            },
            None => {
                return Err(AuthError::NoUser(username));
            }
        }
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
                Some(account) => {
                    return Ok(account);
                },
                None => {
                    return Err(AuthError::NoUser(username));
                }
            }
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
    pub fn print_user(&self, username: &String, password: &String) {
        match self.get(username, password) {
            Ok(account) => {
                println!("\nAccount information for user: {}", account.username);
                println!("\n\tOrders Awaiting Execution");
                for order in account.pending_orders.iter() {
                    println!("\t\t{:?}", order);
                }
                println!("\n\tExecuted Trades");
                for order in account.executed_trades.iter() {
                    println!("\t\t{:?}", order);
                }
                println!("\n");
            },
            Err(e) => Users::print_auth_error(e)
        }
    }

    /* Update this users pending_orders and executed_trades.
     * We have 2 cases to consider, as explained in update_account_orders().
     * */
    fn update_single_user(&mut self, username: &String, trades: &Vec<FilledOrder>) {
        let account = self._get_mut(username).expect("The username wasn't found in the database.");
        // Since we can't remove entries while iterating, store the key's here.
        let mut entries_to_remove: Vec<i32> = Vec::with_capacity(trades.len()); // We know we won't need more than this many entries.
        for trade in trades.iter() {
            match account.pending_orders.get_mut(&trade.id) {
                Some(order) => {
                    if trade.exchanged == (order.quantity - order.filled) {
                        // order completely filled
                        entries_to_remove.push(order.order_id);
                        // account.pending_orders.remove(&order.order_id);
                    } else {
                        // order partially filled
                        order.filled += trade.exchanged;
                    }
                    account.executed_trades.push(trade.clone());
                },
                None => {
                    account.executed_trades.push(trade.clone());
                }
            }
        }

        // Remove all elements from account's hashmap that need to be removed.
        for i in &entries_to_remove {
            account.pending_orders.remove(&i);
        }
    }

    /* Given a vector of Filled Orders, update all the accounts
     * that had orders filled.
     */
    pub fn update_account_orders(&mut self, trades: &Vec<FilledOrder>) {

        /* All orders in the vector were filled by 1 new order,
         * so we have 2 cases to handle.
         * 1. Update all accounts who's orders were filled by new order.
         * 2. Update account of user who's order filled the old orders.
         *
         * */

        let mut update_map: HashMap<String, Vec<FilledOrder>> = HashMap::new();
        let default: Vec<FilledOrder> = Vec::new();
        // Fill update_map
        for trade in trades.iter() {
            let entry = update_map.entry(trade.username.clone()).or_insert(default.clone());
            entry.push(trade.clone());
        }

        // Case 1
        // This is a good candidate for multithreading.
        for (key, val) in update_map.iter() {
            // println!("Updating account: ({})", key);
            self.update_single_user(&key, val);
        }
        // Case 2
        // We need to switch the order type, and the id's.
        let mut swap = trades.clone();
        for trade in swap.iter_mut() {
            let tmp = trade.filled_by;
            trade.filled_by = trade.id;
            trade.id = tmp;
            if trade.action.as_str() == "buy" {
                trade.action = String::from("sell");
            } else {
                trade.action = String::from("buy");
            }
        }
        self.update_single_user(&swap[0].filler_name, &swap);
    }
}
