#[macro_use] extern crate random_number;

pub mod exchange;
pub mod parser;
pub mod account;

pub use crate::exchange::{Exchange, Market, Request};
pub use crate::account::{Users};

use std::env;
use std::process;
use std::io::{self, prelude::*};

fn main() {

    // Our central exchange, everything happens here.
    let mut exchange: Exchange = Exchange::new();
    // All our users are stored here.
    let mut users: Users = Users::new();

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
