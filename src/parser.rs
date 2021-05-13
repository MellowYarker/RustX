pub use crate::exchange::{self, Exchange, Market, Order, InfoRequest, Simulation, Request, PriceError};

/* Prints some helpful information to the console when input is malformed. */
fn malformed_req(req: &String) {
    println!("\nMalformed \"{}\" request!", req);
    println!("Hint - format should be: {} symbol", req);
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
        // Order
        "buy" | "sell" => {
            match words.len() {
                4 => {
                    let order = Order::from( words[0].to_string(),
                                             words[1].to_string().to_uppercase(),
                                             words[2].to_string().trim().parse::<i32>().expect("Please enter an integer number of shares!"),
                                             words[3].to_string().trim().parse::<f64>().expect("Please enter a floating point price!")
                                            );
                    if order.quantity <= 0 || order.price <= 0.0 {
                        println!("Malformed \"{}\" request!", words[0]);
                        println!("Make sure the quantity and price are greater than 0!");
                        return Err(());
                    }
                    return Ok(Request::OrderReq(order));
                },
                _ => {
                    println!("Malformed \"{}\" request!", words[0]);
                    println!("Hint - format should be: {} symbol quantity price", words[0]);
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
                    malformed_req(&words[0]);
                    return Err(());
                }
            }
        },
        // Simulate a market for n time steps
        "simulate" => {
            match words.len() {
                3 => {
                    let req: Simulation = Simulation::from( words[0].to_string(),
                                                            words[1].to_string().to_uppercase(),
                                                            words[2].to_string().trim()
                                                                                .parse::<u32>()
                                                                                .expect("Please enter an integer number of time steps!"));
                    return Ok(Request::SimReq(req));
                },
                _ => {
                    println!("Malformed \"{}\" request!", words[0]);
                    println!("Hint - format shoudl be: {} symbol timesteps", words[0]);
                    return Err(());
                }
            }
        },
        // request instructions
        "help" => {
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
            println!("\t\tEx: history GME\t\t<---- shows past orders that were filled in the GME market.");
            println!("\t\tEx: simulate GME 100\t<---- Simulates 100 random buy/sell orders in the GME market.\n");

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
pub fn service_request(request: Request, exchange: &mut Exchange) {

    match request {
        Request::OrderReq(order) => {
            match &order.action[..] {
                "buy" | "sell" => {
                    // Put the order on the market, it might get filled immediately,
                    // if not it will sit on the market until another order fills it.
                    &exchange.submit_order_to_market(order.clone());
                    &exchange.show_market(&order.security);
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
                    // We have to satisfy the preconditions of the simulation function.
                    let price = exchange.get_price(&req.symbol);
                    match price {
                        Ok(_) => {
                            &exchange.simulate_market(&req);
                        },
                        Err(e) => match e {
                            PriceError::NoMarket => {
                                println!("There is no market for ${}, so we cannot simulate it.", req.symbol);
                            },
                            PriceError::NoTrades => {
                                println!("This market has not executed any trades. Since there is no price information, we cannot simulate it!");
                            }
                        }
                    }
                },
                _ => {
                    println!("I don't know how to handle this Simulation request.");
                }
            }
        }
    }
}
