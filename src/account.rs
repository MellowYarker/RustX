use crate::exchange::requests::Order;
use crate::exchange::filled::FilledOrder;
use std::collections::HashMap;

// Stores data about a user
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

    /* Prints the account information of this user if:
     *  - the account exists and
     *  - the password is correct for this user
     */
    pub fn print_user(&self, username: &String, password: &String) {
        if let Some(account) = self.get(username, password) {
            println!("Orders Awaiting Execution");
            for order in account.placed_orders.iter() {
                println!("{:?}", order);
            }
            println!("\nExecuted Trades");
            for order in account.trades.iter() {
                println!("{:?}", order);
            }
        }
    }
}
