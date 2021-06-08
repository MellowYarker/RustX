pub use crate::exchange::{self, Exchange, Market, Order, InfoRequest, Simulation, CancelOrder, Request, PriceError};
pub use crate::print_instructions;
use postgres::Client;

use crate::account::{UserAccount, Users};

// IO stuff
use std::io::BufReader;
use std::env;
use std::fs::File;

pub struct Argument<R> {
    pub interactive: bool,                      // false means read from file, true means interactive mode
    pub reader: Option<std::io::BufReader<R>>   // The buffer we read from
}

// Parses the command line arguments.
// Returns an argument struct on success, or an error string.
pub fn command_args(mut args: env::Args) -> Result<Argument<std::fs::File>, String> {
    args.next(); // skip the first argument since it's the program name

    // Default argument
    let mut argument = Argument {
        interactive: true,
        reader: None
    };

    // Modify the argument depending on user input.
    match args.next() {
        Some(filename) => {
            let file = match File::open(filename) {
                Ok(f) => f,
                // TODO: pass the error up call stack?
                Err(_) => return Err("Failed to open the file!".to_string())
            };
            argument.interactive = false;
            argument.reader = Some(BufReader::new(file));
        }
        None => ()
    }
    return Ok(argument);
}

/* Prints some helpful information to the console when input is malformed. */
fn malformed_req(req: &str, req_type: &str) {
    eprintln!("\nMalformed \"{}\" request!", req);
    match req_type {
       "account"    => eprintln!("Hint - format should be: {} create/show username password", req),
       "order"      => eprintln!("Hint - format should be: {} symbol quantity price username password", req),
       "cancel"     => eprintln!("Hint - format should be: {} symbol order_id username password", req),
       "info"       => eprintln!("Hint - format should be: {} symbol", req),
       "sim"        => eprintln!("Hint - format should be: {} trader_count market_count duration", req),
       _            => ()
    }
}

/* Takes a string from stdin, and turns it into a Request Enum.
 *
 * If the request does not abide by the required formatting,
 * we return an empty Err, which the caller will handle.
 */
pub fn tokenize_input(text: String) -> Result<Request, ()> {

    // Split the words and reformat them.
    let parsed = text.split_whitespace();
    let mut words = Vec::new();

    for word in parsed {
        words.push(word.to_lowercase());
    }

    // Exit early on empty input
    if words.len() == 0 {
        return Err(());
    }

    // The first entry should be the action type.
    match &(words[0])[..] {
        // Create a new user
        "account" => {
            if let 4 = words.len() {
                let action = words[1].to_string().clone();
                let user = UserAccount::from(&words[2], &words[3]);
                return Ok(Request::UserReq(user, action));
            } else {
                malformed_req(&words[0], &words[0]);
                return Err(());
            }
        }
        // Order
        "buy" | "sell" => {
            if let 6 = words.len() {
                // Note that we do not provide an order ID (arg 4 is None).
                // This value actually gets set later.
                let order = Order::from( words[0].to_string(),
                                         words[1].to_string().to_uppercase(),
                                         words[2].to_string().trim().parse::<i32>().expect("Please enter an integer number of shares!"),// TODO we shouldn't panic here
                                         words[3].to_string().trim().parse::<f64>().expect("Please enter a floating point price!"),     // TODO we shouldn't panic here
                                         None
                                        );
                if order.quantity <= 0 || order.price <= 0.0 {
                    eprintln!("Malformed \"{}\" request!", words[0]);
                    eprintln!("Make sure the quantity and price are greater than 0!");
                    return Err(());
                }
                return Ok(Request::OrderReq(order, words[4].to_string(), words[5].to_string()));
            } else {
                malformed_req(&words[0], "order");
                return Err(());
            }
        },
        "cancel" => {
            if let 5 = words.len() {
                let req = CancelOrder {
                    symbol: words[1].to_string().to_uppercase(),
                    order_id: words[2].to_string().trim().parse::<i32>().expect("Please enter an integer order id"), // TODO we shouldn't panic here.
                    username: words[3].to_string()
                };

                return Ok(Request::CancelReq(req, words[4].to_string()));
            } else {
                malformed_req(&words[0], &words[0]);
                return Err(());
            }
        }
        // request price info, current market info, or past market info
        "price" | "show" | "history" =>  {
            if let 2 = words.len() {
                let req: InfoRequest = InfoRequest::new(words[0].to_string(), words[1].to_string().to_uppercase());
                return Ok(Request::InfoReq(req));
            } else {
                malformed_req(&words[0], "info");
                return Err(());
            }
        },
        // Simulate a market for n time steps
        "simulate" => {
            if let 4 = words.len() {
                // TODO: We shouldn't panic here, just write to stderr...
                let req: Simulation = Simulation::from( words[0].to_string(),
                                                        words[1].to_string().trim().parse::<u32>().expect("Please enter an integer number of traders!"),
                                                        words[2].to_string().trim().parse::<u32>().expect("Please enter an integer number of markets!"),
                                                        words[3].to_string().trim().parse::<u32>().expect("Please enter an integer number of time steps!")
                                                      );
                return Ok(Request::SimReq(req));
            } else {
                malformed_req(&words[0], "sim");
                return Err(());
            }
        },
        // request instructions
        "help" => {
            print_instructions();
            return Err(()); // We return an empty error only because there's no more work to do.
        },
        // Unknown input
        _ => {
            eprintln!("I don't understand the action type \'{}\'.", words[0]);
            return Err(());
        }
    }
}

/* Given a valid Request format, try to execute the Request. */
pub fn service_request(request: Request, exchange: &mut Exchange, users: &mut Users, conn: &mut Client) {
    match request {
        Request::OrderReq(mut order, username, password) => {
            match &order.action[..] {
                "buy" | "sell" => {
                    // Try to get the account
                    match users.authenticate(&username, &password, conn) {
                        Ok(account) => {
                            // Set the order's user id now that we have an account
                            order.user_id = account.id;
                            if account.validate_order(&order) {
                                &exchange.submit_order_to_market(users, order.clone(), &username, true);
                                &exchange.show_market(&order.security);
                            } else {
                                eprintln!("Order could not be placed. This order would fill one of your currently pending orders!");
                            }
                        },
                        Err(e) => Users::print_auth_error(e)
                    }
                },
                // Handle unknown action!
                _ => eprintln!("Sorry, I do not know how to perform {:?}", order)
            }
        },
        Request::CancelReq(order_to_cancel, password) => {
            match users.authenticate(&(order_to_cancel.username), &password, conn) {
                Ok(_) => {
                    match exchange.cancel_order(&order_to_cancel, users) {
                        Ok(_) => println!("Order successfully cancelled."),
                        Err(e) => eprintln!("{}", e)
                    }
                },
                Err(e) => Users::print_auth_error(e)
            }

        },
        Request::InfoReq(req) => {
            match &req.action[..] {
                // We've requested the price of a security.
                "price" => {
                    let price = exchange.get_price(&req.symbol);
                    match price {
                        Ok(price) => {
                            println!("Last trading price of ${} is ${}", req.symbol, price);
                        },
                        Err(e) => match e {
                            PriceError::NoMarket => {
                                println!("There is no market for ${}, so no price information exists.", req.symbol);
                            },
                            PriceError::NoTrades => {
                                println!("This market has not had any trades yet, so there is no price!");
                            }
                        }
                    }
                },
                // Show the current market.
                "show" => {
                    if exchange.statistics.contains_key(&req.symbol) {
                        exchange.show_market(&req.symbol);
                    } else {
                        println!("Sorry, we have no market information on ${}", req.symbol);
                    }
                },
                // Show the past orders of this market.
                "history" => {
                    if exchange.trades.contains_key(&req.symbol) {
                        exchange.show_market_history(&req.symbol);
                    } else {
                        println!("The symbol that was requested either doesn't exist or has no past trades.");
                    }
                },
                _ => {
                    eprintln!("I don't know how to handle this information request.");
                }
            }
        },
        Request::SimReq(req) => {
            match &req.action[..] {
                "simulate" => {
                    println!("Simulating {} order(s) in {} market(s) among {} account(s)!", req.duration, req.market_count, req.trader_count);
                    &exchange.simulate_market(&req, users);
                },
                _ => {
                    eprintln!("I don't know how to handle this Simulation request.");
                }
            }
        },
        Request::UserReq(account, action) => {
            match &action[..] {
                "create" => {
                   // match users.new_account(account) {
                   match users.new_account(account, conn) {
                       Some(id) => println!("Successfully created new account with id {}.", id),
                       None => println!("Sorry, that username is already taken!")
                   }
                },
                "show" => {
                    match users.authenticate(&account.username, &account.password, conn) {
                        Ok(_) => {
                            users.print_user(&account.username, true);
                        },
                        Err(e) => Users::print_auth_error(e)
                    }
                },
                _ => println!("Sorry I do not know how to handle that account request.")
            }
        }
    }
}
