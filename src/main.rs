#[macro_use] extern crate random_number;

pub mod exchange;
pub mod parser;
pub mod account;

pub use crate::exchange::{Exchange, Market, Request};
pub use crate::parser::{tokenize_input, service_request};

pub use crate::account::{Users};

use std::io::{self, prelude::*, BufReader};
use std::env;
use std::fs::File;

fn main() {

    // Our central exchange, everything happens here.
    let mut exchange: Exchange = Exchange::new();
    // All our users are stored here.
    let mut users: Users = Users::new();

    let args: Vec<String> = env::args().collect();

    // Read from file
    if args.len() == 2 {
        let filename = &args[1];
        let file = File::open(filename).expect("Some error in reading the file.");
        let reader = BufReader::new(file);

        for line in reader.lines() {
            match line {
                Ok(input) => {
                    let keep = input.clone();
                    let request: Request = match tokenize_input(input) {
                        // Ok(req) => req,
                        Ok(req) => {
                            req
                        },
                        Err(_)  => {
                            println!("Please enter a valid request.");
                            continue;
                        }
                    };

                    // Our input has been validated, and we can now
                    // attempt to service the request.
                    println!("Request: {}", keep);
                    service_request(request, &mut exchange, &mut users);
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

        let buy_price = 167.34;
        let buy_amount = 24;
        let sell_price = 999.85;
        let sell_amount = 12;
        println!("Usage:");
        println!("\tOrders: ACTION SYMBOL(ticker) QUANTITY PRICE");
        println!("\t\tEx: BUY GME {} {}\t<---- Sends a buy order for {} shares of GME at ${} a share.", buy_amount, buy_price, buy_amount, buy_price);
        println!("\t\tEx: SELL GME {} {}\t<---- Sends a sell order for {} shares of GME at ${} a share.\n", sell_amount, sell_price, sell_amount, sell_price);

        println!("\tInfo Requests: ACTION SYMBOL(ticker)");
        println!("\t\tEx: price GME\t\t<---- gives latest price an order was filled at.");
        println!("\t\tEx: show GME\t\t<---- shows statistics for the GME market.");
        println!("\t\tEx: history GME\t\t<---- shows past orders that were filled in the GME market.\n");

        println!("\tSimulation Requests: simulate SYMBOL(ticker) NUM_ORDERS");
        println!("\t\tEx: simulate GME 100\t<---- Simulates 100 random buy/sell orders in the GME market.\n");

        println!("\tCreate new user: create USERNAME PASSWORD");
        println!("\t\tEx: create bigMoney notHashed\n");
        println!("\tYou can see these instructions at any point by typing help.");

        loop {
            println!("\n---What would you like to do?---\n");

            let mut input = String::new(); // mutable

            io::stdin()
                .read_line(&mut input)
                    .expect("Failed to read line");

            let request: Request = match tokenize_input(input) {
                Ok(req) => req,
                Err(_)  => {
                    println!("Please enter a valid request.");
                    continue;
                }
            };

            // Our input has been validated, and we can now
            // attempt to service the request.
            service_request(request, &mut exchange, &mut users);
        }
    }
}
