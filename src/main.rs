#[macro_use] extern crate random_number;

pub mod exchange;
pub mod parser;
pub mod account;
pub mod database;

pub use crate::exchange::{Exchange, Market, Request};
pub use crate::account::{Users};

use std::env;
use std::process;
use std::io::{self, prelude::*};

use postgres::{Client, NoTls};

fn main() {
    // Our central exchange, everything happens here.
    let mut exchange: Exchange = Exchange::new();
    // All our users are stored here.
    let mut users: Users = Users::new();

    // Read in the users from the database.
    // TODO: Which users should be read in?
    //       We probably don't want to have *every* user,
    //       only the one's who are likely to be placing orders.
    //
    //       That, or we can read and maintain users as they request info.
    let mut client = Client::connect("host=localhost user=postgres dbname=mydb", NoTls).expect("Failed to connect to Database. Please ensure it is up and running.");
    println!("Successful connection!");
    for row in client.query("SELECT id, username, password FROM Account", &[]).expect("Something went wrong in the query.") {
        let id: i32 = row.get(0);
        let username: &str = row.get(1);
        let password: &str = row.get(2);

        println!("found account: {}, {}, {}", id, username, password);

        // TODO: Insert the users we found into the Users hashmap.
    }

    /* TODO
     *  We need to populate our exchange with the relevant data from the database.
     *  Data we care about includes:
     *      - Top N buys and sells in each market
     *      - Current statistics for every market
     * */
    // Fill the pending orders of the markets
    database::populate_exchange_markets(&mut exchange, &mut client);
    // Fill the statistics for each market
    database::populate_market_statistics(&mut exchange, &mut client);
    // Fill the statistics for the exchange
    database::populate_exchange_statistics(&mut exchange, &mut client);

    let argument = match parser::command_args(env::args()) {
        Ok(arg) => arg,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    // Read from file mode
    if !argument.interactive {
        for line in argument.reader.unwrap().lines() {
            match line {
                Ok(input) => {
                    let raw = input.clone();
                    let request: Request = match parser::tokenize_input(input) {
                        Ok(req) => req,
                        Err(_)  => {
                            println!("WARNING: [{}] is not a valid request.", raw);
                            continue;
                        }
                    };

                    // Our input has been validated, and we can now
                    // attempt to service the request.
                    println!("Servicing Request: {}", raw);
                    parser::service_request(request, &mut exchange, &mut users);
                },
                Err(_) => return
            }
        }
    } else {
        // User interface version
        println!("
         _       __     __                             __           ____             __ _  __
         | |     / /__  / /________  ____ ___  ___     / /_____     / __ \\__  _______/ /| |/ /
         | | /| / / _ \\/ / ___/ __ \\/ __ `__ \\/ _ \\   / __/ __ \\   / /_/ / / / / ___/ __/   /
         | |/ |/ /  __/ / /__/ /_/ / / / / / /  __/  / /_/ /_/ /  / _, _/ /_/ (__  ) /_/   |
         |__/|__/\\___/_/\\___/\\____/_/ /_/ /_/\\___/   \\__/\\____/  /_/ |_|\\__,_/____/\\__/_/|_|\n");


        print_instructions();
        loop {
            println!("\n---What would you like to do?---\n");

            let mut input = String::new();

            io::stdin()
                .read_line(&mut input)
                    .expect("Failed to read line");

            let request: Request = match parser::tokenize_input(input) {
                Ok(req) => req,
                Err(_)  => continue
            };

            // Our input has been validated, and we can now
            // attempt to service the request.
            parser::service_request(request, &mut exchange, &mut users);
        }
    }
}

pub fn print_instructions() {
    let buy_price = 167.34;
    let buy_amount = 24;
    let sell_price = 999.85;
    let sell_amount = 12;
    let user = "example";
    let pass = "pass";

    println!("Usage:");
    println!("\tOrders: ACTION(buy/sell) SYMBOL(ticker) QUANTITY PRICE USERNAME PASSWORD");
    println!("\t\tEx: buy GME {} {} {} {}\t<---- Sends a buy order for {} shares of GME at ${} a share. Order is placed by {} with password {}.", buy_amount, buy_price, user, pass, buy_amount, buy_price, user, pass);
    println!("\t\tEx: sell GME {} {} {} {}\t<---- Sends a sell order for {} shares of GME at ${} a share. Order is placed by {} with password {}.\n", sell_amount, sell_price, user, pass, sell_amount, sell_price, user, pass);

    println!("\tCancel Request: cancel SYMBOL ORDER_ID USERNAME PASSWORD");
    println!("\t\tEx: cancel AAPL 4 admin pass\t\t<---- Cancels the order with ID 4 in the AAPL market, provided user (admin) placed it.\n");

    println!("\tInfo Requests: ACTION SYMBOL(ticker)");
    println!("\t\tEx: price GME\t\t<---- gives latest price an order was filled at.");
    println!("\t\tEx: show GME\t\t<---- shows statistics for the GME market.");
    println!("\t\tEx: history GME\t\t<---- shows past orders that were filled in the GME market.\n");

    println!("\tSimulation Requests: simulate NUM_USERS NUM_MARKETS NUM_ORDERS");
    println!("\t\tEx: simulate 300 500 10000\t<---- Simulates 10000 random buy/sell orders in 500 markets, with 300 random users.\n");

    println!("\tAccount Requests: account create/show USERNAME PASSWORD");
    println!("\t\tEx: account create bigMoney notHashed\n");
    println!("\tYou can see these instructions at any point by typing help.");
}
