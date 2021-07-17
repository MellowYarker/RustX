#[macro_use] extern crate random_number;
extern crate chrono;
extern crate ctrlc;
extern crate redis;
#[macro_use] extern crate colour;

pub mod exchange;
pub mod parser;
pub mod account;
pub mod buffer;
pub mod database;

pub use crate::exchange::{Exchange, Market, Request};
pub use crate::account::{Users};
pub use crate::buffer::{BufferCollection, UpdateCategories};

use std::env;
use std::process;
use std::io::{self, prelude::*};

use postgres::{Client, NoTls};

use std::thread;
use std::sync::mpsc;

use std::time::Instant;


// Helps us determine what each thread will work on.
pub enum Category {
    InsertNew,
    UpdateKnown,
    InsertPending,
    DeletePending,
    UpdateTotal,
    UpdateMarketStats,
    InsertNewTrades
}

// Helps manage the workload.
pub struct WorkerThreads<T> {
    pub threads: Vec<thread::JoinHandle<T>>, // Holds thread handles
    pub senders: Vec<mpsc::Sender<(UpdateCategories, Category)>>, // Each thread receives only 1 category, all others are empty.
    pub receivers: Vec<mpsc::Receiver<bool>> // When a thread is finished flushing it's category, it writes `true` to the channel.
}

fn main() {
    let mut exchange = Exchange::new();  // Our central exchange, everything happens here.
    let mut users    = Users::new();     // All our users are stored here.
    let mut buffers  = BufferCollection::new(200000, 200000); // In-memory buffers that will batch write to DB.

    let mut client = Client::connect("host=localhost user=postgres dbname=rustx", NoTls)
        .expect("Failed to connect to Database. Please ensure it is up and running.");

    dark_green!("Connected to database.\n");

    let start = Instant::now();
    let user_count = Instant::now();
    // Reads total # users
    users.direct_update_total(&mut client);
    let user_count = user_count.elapsed().as_millis();

    /* TODO: Should we store the top N buys and sells in each market, rather than all?
     *       This would decrease the amount of RAM, and increases the computation speed.
     *       I think this needs to wait for a move to Redis, as we currently read users
     *       pending orders into their accounts by pulling this data
     *          - (see fetch_account_pending_orders).
     **/
    println!("Initializing exchange...");
    dark_green!("\tTime elapsed to get user count: {} ms\n", user_count);
    let market_time = Instant::now();
    database::populate_exchange_markets(&mut exchange, &mut client);    // Fill the pending orders of the markets
    let market_time = market_time.elapsed().as_millis();
    dark_green!("\tTime elapsed to populate markets: {} ms\n", market_time);

    let stats_time = Instant::now();
    database::populate_market_statistics(&mut exchange, &mut client);   // Fill the statistics for each market
    let stats_time = stats_time.elapsed().as_millis();
    dark_green!("\tTime elapsed to populate market stats: {} ms\n", stats_time);

    let x_stats_time = Instant::now();
    database::populate_exchange_statistics(&mut exchange, &mut client); // Fill the statistics for the exchange
    let x_stats_time = x_stats_time.elapsed().as_millis();
    dark_green!("\tTime elapsed to populate exchange stats: {} ms\n", x_stats_time);

    let has_trades_time = Instant::now();
    database::populate_has_trades(&mut exchange, &mut client);          // Fill the has_trades map for the exchange
    let has_trades_time = has_trades_time.elapsed().as_millis();
    dark_green!("\tTime elapsed to populate has_trades: {} ms\n", has_trades_time);

    let end = start.elapsed().as_millis();
    dark_green!("\nTotal Setup Time elapsed : {} ms\n", end);

    let argument = match parser::command_args(env::args()) {
        Ok(arg) => arg,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    // Set sigINT/sigTERM handlers
    // TODO: If we want the sigINT handler thread to be capable of flushing the buffers, we'll need
    // to share the buffers with it. To do this, we will have to wrap the buffers inside a mutex
    // and wrap the mutex in an Arc.
    //
    // This might not be too technically difficult, but I'm not sure I like the behaviour:
    //  -  It implies that we can shut the exchange while an order is being processed, potentially
    //     resulting in inconsistent state.
    //  -  To solve this, we would have to have some other shared var that says the state is
    //     consistent, and since we're shutting down no more orders can be placed.
    ctrlc::set_handler(|| {
        println!("Please use the EXIT command, still figuring out how to do a controlled shutdown...");
    }).expect("Error setting Ctrl-C handler");


    let (tx, rx) = mpsc::channel();
    buffers.set_transmitter(tx);

    /* This thread's job is to read categorized buffer data and write it to the database.
     *
     * It behaves in the following way:
     *  1. Set up worker threads and additional channels
     *  loop {
     *      2.  Read the categories, if we got None, we must shutdown immediately.
     *      3.  If we got Some(data), send each component to the appropriate worker thread
     *          to be written to the database.
     *  }
     *
     **/
    let handler = thread::spawn(move || {

        let mut workers = WorkerThreads {
            threads: Vec::new(),
            senders: Vec::new(),
            receivers: Vec::new()
        };

        // These are our worker threads. The buffer handling thread
        // will write each category to its respective worker thread to be
        // written to the database.
        for _ in 0..7 {
            // Set up the transmitter x receiver channel for sending data to worker,
            // then set up response channel to get `true` message of completion.
            let (transmitter, receiver) = mpsc::channel();
            let (response_tx, response_rx) = mpsc::channel();
            workers.senders.push(transmitter);
            workers.receivers.push(response_rx);

            let mut conn = Client::connect("host=localhost user=postgres dbname=rustx", NoTls)
                .expect("Failed to connect to Database. Please ensure it is up and running.");

            workers.threads.push(thread::spawn(move || {
                loop {
                    let (data, category_type): (UpdateCategories, Category) = match receiver.recv() {
                        Ok((data, category_type)) => (data, category_type),
                        Err(_) => {
                            return;
                        }
                    };

                    // Perform the database write here depending on the type of category.
                    match category_type {
                        Category::InsertNew            => BufferCollection::launch_insert_orders(&data.insert_orders, &mut conn),
                        Category::UpdateKnown          => BufferCollection::launch_update_orders(&data.update_orders, &mut conn),
                        Category::InsertPending        => BufferCollection::launch_insert_pending_orders(&data.insert_pending, &mut conn),
                        Category::DeletePending        => BufferCollection::launch_delete_pending_orders(&data.delete_pending, &mut conn),
                        Category::UpdateTotal          => BufferCollection::launch_exchange_stats_update(data.total_orders, &mut conn),
                        Category::UpdateMarketStats    => BufferCollection::launch_update_market(&data.update_markets, &mut conn),
                        Category::InsertNewTrades      => BufferCollection::launch_insert_trades(&data.insert_trades, &mut conn)
                    }
                    // Return the successful response message
                    response_tx.send(true).unwrap();
                }
            }));
        }

        // This is the main loop for the Buffer handling thread.
        // We read the categories from the main thread, then send them
        // to the worker threads. On shutdown, we clean everything up.
        loop {
            let categories: UpdateCategories = match rx.recv() {
                Ok(option) => match option {
                    Some(data) => data,
                    // We write None to channel on shutdown.
                    // Better way would be to close Sender, but I'm having trouble with that...
                    None => {
                        dark_blue!("[Buffer Thread]: received shutdown request.\n");
                        drop(rx);
                        dark_blue!("[Buffer Thread]: waiting on worker threads to complete...\n");

                        for tx in workers.senders {
                            drop(tx);
                        }
                        for handle in workers.threads {
                            handle.join().unwrap();
                        }
                        return;
                    }
                },
                Err(_) => {
                    return;
                }
            };

            dark_blue!("[BUFFER THREAD]: Initiating database writes.\n");
            BufferCollection::launch_batch_db_updates(&categories, &mut workers);
            dark_blue!("[BUFFER THREAD]: Writes successfully flushed.\n");
        }
    });

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
                    // If we got an exit request, exit the loop and treat it like EOF.
                    if let Request::ExitReq = request {
                        break;
                    }

                    // Our input has been validated. We can now attempt to service the request.
                    parser::service_request(request, &mut exchange, &mut users, &mut buffers, &mut client);
                },
                Err(_) => return
            }

            // Make sure our buffer states are accurate.
            buffers.update_buffer_states();
            // If order buffer was drained, we can reset our cached values modified field.
            if buffers.transmit_buffer_data(&exchange) {
                users.reset_users_modified();
                // Set all market stats modified to false
                for (_key, entry) in exchange.statistics.iter_mut() {
                    entry.modified = false;
                }
            }
        }

        let exit = Request::ExitReq;
        parser::service_request(exit, &mut exchange, &mut users, &mut buffers, &mut client);
    } else {
        // User interface version
        dark_yellow!("
         _       __     __                             __           ____             __ _  __
         | |     / /__  / /________  ____ ___  ___     / /_____     / __ \\__  _______/ /| |/ /
         | | /| / / _ \\/ / ___/ __ \\/ __ `__ \\/ _ \\   / __/ __ \\   / /_/ / / / / ___/ __/   /
         | |/ |/ /  __/ / /__/ /_/ / / / / / /  __/  / /_/ /_/ /  / _, _/ /_/ (__  ) /_/   |
         |__/|__/\\___/_/\\___/\\____/_/ /_/ /_/\\___/   \\__/\\____/  /_/ |_|\\__,_/____/\\__/_/|_|\n");


        print_instructions();
        loop {
            dark_yellow!("\n---What would you like to do?---\n");

            let mut input = String::new();

            io::stdin()
                .read_line(&mut input)
                    .expect("Failed to read line");

            let request: Request = match parser::tokenize_input(input) {
                Ok(req) => req,
                Err(_)  => continue
            };

            // If we got an exit request, service it and exit loop.
            if let Request::ExitReq = request {
                parser::service_request(request, &mut exchange, &mut users, &mut buffers, &mut client);
                break;
            }

            // Our input has been validated. We can now attempt to service the request.
            parser::service_request(request, &mut exchange, &mut users, &mut buffers, &mut client);

            // Make sure our buffer states are accurate.
            buffers.update_buffer_states();
            // If order buffer was drained, we can reset our cached values modified field.
            if buffers.transmit_buffer_data(&exchange) {
                users.reset_users_modified();

                // Set all market stats modified to false
                for (_key, entry) in exchange.statistics.iter_mut() {
                    entry.modified = false;
                }
            }
        }
    }

    // Wait for the buffer thread to complete.
    handler.join().unwrap();
    println!("\nShutdown sequence complete. Goodbye!");
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
