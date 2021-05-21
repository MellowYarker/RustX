pub use crate::exchange::{self, Exchange, Market, Order, InfoRequest, Simulation, Request, PriceError};
pub use crate::print_instructions;

// pub mod account;
use crate::account::{UserAccount, Users};

/* Prints some helpful information to the console when input is malformed. */
fn malformed_req(req: &str, req_type: &str) {
    println!("\nMalformed \"{}\" request!", req);
    match req_type {
       "account" => println!("Hint - format should be: {} create/show username password", req),
       "order"  => println!("Hint - format should be: {} symbol quantity price username password", req),
       "info"   => println!("Hint - format should be: {} symbol", req),
       "sim"    => println!("Hint - format should be: {} trader_count market_count duration", req),
       _        => ()
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
            match words.len() {
                4 => {
                    let action = words[1].to_string().clone();
                    let user = UserAccount::from(&words[2], &words[3]);
                    return Ok(Request::UserReq(user, action));
                },
                _ => {
                    malformed_req(&words[0], &words[0]);
                    return Err(());
                }
            }
        }
        // Order
        "buy" | "sell" => {
            match words.len() {
                6 => {
                    let order = Order::from( words[0].to_string(),
                                             words[1].to_string().to_uppercase(),
                                             words[2].to_string().trim().parse::<i32>().expect("Please enter an integer number of shares!"),
                                             words[3].to_string().trim().parse::<f64>().expect("Please enter a floating point price!"),
                                             &words[4].to_string()
                                            );
                    if order.quantity <= 0 || order.price <= 0.0 {
                        println!("Malformed \"{}\" request!", words[0]);
                        println!("Make sure the quantity and price are greater than 0!");
                        return Err(());
                    }
                    return Ok(Request::OrderReq(order, words[4].to_string(), words[5].to_string()));
                },
                _ => {
                    malformed_req(&words[0], "order");
                    return Err(());
                }
            }
        },
        // request price info, current market info, or past market info
        "price" | "show" | "history" =>  {
            match words.len() {
                2 => {
                    let req: InfoRequest = InfoRequest::new(words[0].to_string(), words[1].to_string().to_uppercase());
                    return Ok(Request::InfoReq(req));
                },
                _ =>  {
                    malformed_req(&words[0], "info");
                    return Err(());
                }
            }
        },
        // Simulate a market for n time steps
        "simulate" => {
            match words.len() {
                4 => {
                    let req: Simulation = Simulation::from( words[0].to_string(),
                                                            words[1].to_string().trim()
                                                                                .parse::<u32>()
                                                                                .expect("Please enter an integer number of traders!"),
                                                            words[2].to_string().trim()
                                                                                .parse::<u32>()
                                                                                .expect("Please enter an integer number of markets!"),
                                                            words[3].to_string().trim()
                                                                                .parse::<u32>()
                                                                                .expect("Please enter an integer number of time steps!"));
                    return Ok(Request::SimReq(req));
                },
                _ => {
                    malformed_req(&words[0], "sim");
                    return Err(());
                }
            }
        },
        // request instructions
        "help" => {
            print_instructions();
            return Err(()); // We return an empty error only because there's no more work to do.
        },
        // Unknown input
        _ => {
            println!("I don't understand the action type \'{}\'.", words[0]);
            return Err(());
        }
    }
}

/* Given a valid Request format, try to execute the Request. */
pub fn service_request(request: Request, exchange: &mut Exchange, users: &mut Users) {
    match request {
        Request::OrderReq(order, username, password) => {
            match &order.action[..] {
                "buy" | "sell" => {
                    // Try to get the account
                    match users.authenticate(&username, &password) {
                        Ok(account) => {
                            if account.validate_order(&order) {
                                &exchange.submit_order_to_market(users, order.clone(), &username, true);
                                &exchange.show_market(&order.security);
                            } else {
                                println!("Order could not be placed. This order would fill one of your currently pending orders!");
                            }
                        },
                        Err(e) => Users::print_auth_error(e)
                    }
                },
                // Handle unknown action!
                _ => println!("Sorry, I do not know how to perform {:?}", order)
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
                    if exchange.filled_orders.contains_key(&req.symbol) {
                        exchange.show_market_history(&req.symbol);
                    } else {
                        println!("The symbol that was requested either doesn't exist or has no past trades.");
                    }
                },
                _ => {
                    println!("I don't know how to handle this information request.");
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
                    println!("I don't know how to handle this Simulation request.");
                }
            }
        },
        Request::UserReq(account, action) => {
            match &action[..] {
                "create" => {
                   match users.new_account(account) {
                       Some(id) => println!("Successfully created new account with id {}.", id),
                       None => println!("Sorry, that username is already taken!")
                   }
                },
                "show" => {
                    match users.authenticate(&account.username, &account.password) {
                        Ok(_) => {
                            // TODO: Figure out authentication because this is dumb.
                            users.print_user(&account.username, &account.password);
                        },
                        Err(e) => Users::print_auth_error(e)
                    }
                },
                _ => println!("Sorry I do not know how to handle that account request.")
            }
        }
    }
}
