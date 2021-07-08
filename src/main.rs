#[macro_use] extern crate random_number;
extern crate chrono;
extern crate ctrlc;

pub mod exchange;
pub mod parser;
pub mod account;
pub mod buffer;
pub mod database;

pub use crate::exchange::{Exchange, Market, Request};
pub use crate::account::{Users};
pub use crate::buffer::BufferCollection;

use std::env;
use std::process;
use std::io::{self, prelude::*};

use postgres::{Client, NoTls};

use std::time::Instant;

fn main() {
    let mut exchange = Exchange::new();  // Our central exchange, everything happens here.
    let mut users    = Users::new();     // All our users are stored here.
    let mut buffers  = BufferCollection::new(200000, 200000); // In-memory buffers that will batch write to DB.

    let mut client = Client::connect("host=localhost user=postgres dbname=rustx", NoTls)
        .expect("Failed to connect to Database. Please ensure it is up and running.");

    println!("Connected to database.");

    let start = Instant::now();
    let user_count = Instant::now();
    // Reads total # users
    users.direct_update_total(&mut client);
    let user_count = user_count.elapsed().as_millis();

    /* TODO: Top N buys and sells in each market, rather than all.
     *       This decreases the amount of RAM, increases the computation speed.
     **/
    println!("Getting markets.");
    let market_time = Instant::now();
    database::populate_exchange_markets(&mut exchange, &mut client);    // Fill the pending orders of the markets
    let market_time = market_time.elapsed().as_millis();
    let stats_time = Instant::now();
    database::populate_market_statistics(&mut exchange, &mut client);   // Fill the statistics for each market
    let stats_time = stats_time.elapsed().as_millis();
    let x_stats_time = Instant::now();
    database::populate_exchange_statistics(&mut exchange, &mut client); // Fill the statistics for the exchange
    let x_stats_time = x_stats_time.elapsed().as_millis();
    let has_trades_time = Instant::now();
    database::populate_has_trades(&mut exchange, &mut client);          // Fill the has_trades map for the exchange
    let has_trades_time = has_trades_time.elapsed().as_millis();

    let end = start.elapsed().as_millis();
    println!("Populated users, markets, and statistics.");
    println!("\tTime elapsed to get user count: {} ms", user_count);
    println!("\tTime elapsed to populate markets: {} ms", market_time);
    println!("\tTime elapsed to populate market stats: {} ms", stats_time);
    println!("\tTime elapsed to populate exchange stats: {} ms", x_stats_time);
    println!("\tTime elapsed to populate has_trades: {} ms", has_trades_time);
    println!("\nTotal Setup Time elapsed : {} ms", end);

    let argument = match parser::command_args(env::args()) {
        Ok(arg) => arg,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    // Set sigINT/sigTERM handlers
    // TODO: Apparently we can't just read the mut ref.
    //       Rustc thinks that the buffer, exchange, and client may go out of scope
    //       before this thread triggers the flush.
    ctrlc::set_handler(|| {
        println!("Please use the EXIT command, still figuring out how to do a controlled shutdown...");
    }).expect("Error setting Ctrl-C handler");

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

                    println!("Servicing Request: {}", raw);
                    // Our input has been validated. We can now attempt to service the request.
                    parser::service_request(request, &mut exchange, &mut users, &mut buffers, &mut client);
                },
                Err(_) => return
            }

            // Make sure our buffer states are accurate.
            if buffers.update_buffer_states(&exchange, &mut client) {
                users.reset_users_modified();
                // Set all market stats modified to false
                for (_key, entry) in exchange.statistics.iter_mut() {
                    entry.modified = false;
                }
            }
        }

        buffers.flush_on_shutdown(&exchange, &mut client);
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

            // If we got an exit request, service it an exit.
            if let Request::ExitReq = request {
                parser::service_request(request, &mut exchange, &mut users, &mut buffers, &mut client);
                return;
            }

            // Our input has been validated. We can now attempt to service the request.
            parser::service_request(request, &mut exchange, &mut users, &mut buffers, &mut client);

            // Make sure our buffer states are accurate.
            if buffers.update_buffer_states(&exchange, &mut client) {
                users.reset_users_modified();

                // Set all market stats modified to false
                for (_key, entry) in exchange.statistics.iter_mut() {
                    entry.modified = false;
                }
            }
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
    println!("\t\tEx: account create bigMoney notHashed\n\n");
    println!("\tTo perform a graceful shutdown and update the database, type EXIT.\n");
    println!("\tYou can see these instructions at any point by typing help.");
}
