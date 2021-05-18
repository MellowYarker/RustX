use crate::exchange::requests::Order;
use crate::exchange::filled::FilledOrder;
use std::collections::HashMap;

// Stores data about a user
#[derive(Debug)]
pub struct UserAccount {
    pub username: String,
    pub password: String,
    pub id: Option<i32>,
    pub placed_orders: Vec<Order>,
    pub trades: Vec<FilledOrder>
}

// Where we store all our users
pub struct Users {
    pub users: HashMap<String, UserAccount>,
    pub total: i32
}

impl UserAccount {
    pub fn from(name: &String, password: &String) -> Self {
        let placed: Vec<Order> = Vec::new();
        let trades: Vec<FilledOrder> = Vec::new();
        UserAccount {
            username: name.to_string().clone(),
            password: password.to_string().clone(),
            id: None, // We set this later
            placed_orders: placed,
            trades: trades,
        }
    }

    // Sets this account's user id, and returns it.
    pub fn set_id(&mut self, users: &Users) -> i32 {
        self.id = Some(users.total + 1);
        return self.id.unwrap();
    }
}

impl Users {
    /* Returns true if authentication succeeded,
     * false if username doesn't exist or if password is wrong.
     */
    pub fn authenticate(&self, username: &String, password: &String) -> bool {
        match self.users.get(username) {
            Some(account) => {
                if *password == account.password {
                    return true;
                }
            },
            None => ()
        }
        return false;
    }
    pub fn new() -> Self {
        let map: HashMap<String, UserAccount> = HashMap::new();
        Users {
            users: map,
            total: 0
        }
    }

    // If an account with this username exists, exit early, otherwise
    // add the account and return it's ID.
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
    pub fn get(&self, username: &String, password: &String) -> Option<&UserAccount> {
        match self.users.get(username) {
            Some(account) => {
                if *password == account.password {
                    return Some(account);
                }
                println!("Incorrect password for username ({})", username);
                return None;
            },
            None => {
                println!("The provided username doesn't exist in our records.");
                return None;
            }
        }
    }

    /* Returns a mutable reference to a user account if:
     *  - the account exists and
     *  - the password is correct for this user
     */
    pub fn get_mut(&mut self, username: &String, password: &String) -> Option<&mut UserAccount> {
        match self.users.get_mut(username) {
            Some(account) => {
                if *password == account.password {
                    return Some(account);
                }
                println!("Incorrect password for username ({})", username);
                return None;
            },
            None => {
                println!("The provided username doesn't exist in our records.");
                return None;
            }
        }
    }

    /* Returns a mutable reference to a user account if:
     *  - the account exists
     */
    pub fn _get_mut(&mut self, username: &String) -> Option<&mut UserAccount> {
        match self.users.get_mut(username) {
            Some(account) => {
                return Some(account);
            },
            None => {
                return None;
            }
        }
    }

    /* Prints the account information of this user if:
     *  - the account exists and
     *  - the password is correct for this user
     */
    pub fn print_user(&self, username: &String, password: &String) {
        if let Some(account) = self.get(username, password) {
            println!("Results for user: {}\n", account.username);
            println!("\tOrders Awaiting Execution");
            for order in account.placed_orders.iter() {
                println!("\t\t{:?}", order);
            }
            println!("\n\tExecuted Trades");
            for order in account.trades.iter() {
                println!("\t\t{:?}", order);
            }
        }
    }

    /* Update this users placed_orders and trades.
     * We have 2 cases to consider, as explained in update_account_orders().
     * */
    fn update_single_user(&mut self, username: &String, trades: &Vec<FilledOrder>) {
        let account = self._get_mut(username).expect("The username wasn't found in the database.");
        for trade in trades.iter() {
            match account.placed_orders.binary_search_by(|probe: &Order| probe.order_id.cmp(&trade.id)){
                Ok(index) => {
                    // The order that was filled was found in the accounts
                    // pending orders.
                    let order: &mut Order = &mut account.placed_orders[index];
                    if trade.exchanged == (order.quantity - order.filled) {
                        // order completely filled
                        account.placed_orders.remove(index);
                    } else {
                        // order partially filled
                        order.filled += trade.exchanged;
                    }
                    account.trades.push(trade.clone());
                },
                Err(_) => {
                    // The trade wasn't found in placed_orders because
                    // it was completely filled before being placed on the market.
                    account.trades.push(trade.clone());
                }
            }
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
