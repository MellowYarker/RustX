use crate::exchange::requests::Order;
use std::collections::HashMap;

// Stores data about a user
pub struct UserAccount {
    username: String,
    password: String,
    id: Option<i32>,
    buys: Vec<Order>,
    sells: Vec<Order>
}

// Where we store all our users
pub struct Users {
    users: HashMap<String, UserAccount>,
    total: i32
}

impl UserAccount {
    pub fn from(name: &String, password: &String) -> Self {
        let vec: Vec<Order> = Vec::new();
        UserAccount {
            username: name.to_string().clone(),
            password: password.to_string().clone(),
            id: None, // Set later
            buys: vec.clone(),
            sells: vec,
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
}
