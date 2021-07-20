use crate::exchange::Order;
use crate::exchange::filled::Trade;
// Statistics about a security
#[derive(Debug, Clone)]
pub struct SecStat {
    pub symbol: String,
    pub total_buys: i32,
    pub total_sells: i32,
    pub filled_buys: i32,
    pub filled_sells: i32,
    pub last_price: Option<f64>, // Last price we got
    pub modified: bool
}

impl SecStat {
    pub fn from(order: &Order) -> Self {

        let symbol = order.symbol.clone();

        let total_buys = match &order.action[..] {
            "BUY" => 1,
            _ => 0
        };

        let total_sells  = match &order.action[..] {
            "SELL" => 1,
            _ => 0
        };

        let last_price = None;

        SecStat {
            symbol: symbol,
            total_buys: total_buys,
            total_sells: total_sells,
            filled_buys: 0,
            filled_sells: 0,
            last_price: last_price,
            modified: false
        }
    }

    pub fn direct(symbol: &str, total_buys: i32, total_sells: i32, filled_buys: i32, filled_sells: i32, last_price: Option<f64>) -> Self {

        let mut last_price = last_price;

        if let Some(price) = last_price {
            last_price = Some(f64::trunc(price * 100.0) / 100.0);
        };

        SecStat {
            symbol: symbol.to_string().clone(),
            total_buys,
            total_sells,
            filled_buys,
            filled_sells,
            last_price,
            modified: false
        }
    }

    pub fn update_market_stats(&mut self, new_price: f64, trades: &Vec<Trade>) {
        // Update our in-memory data-structures.
        // Calling function will update db.
        self.update_price(new_price);
        self.update_trades(trades);
    }

    // Updates the price, returns the difference.
    fn update_price(&mut self, new_price: f64) -> f64 {
        match self.last_price {
            Some(price) => {
                let diff = price - new_price;
                self.last_price = Some(new_price);
                return diff;
            },
            None => {
                self.last_price = Some(new_price);
                return new_price;
            }
        }
    }

    // Iterates over the vector of trades and
    // updates the filled buy or sell count.
    fn update_trades(&mut self, trades: &Vec<Trade>) {
        for order in trades {
            match &order.action[..] {
                "BUY" => {
                    self.filled_buys += 1;
                },
                "SELL" => {
                    self.filled_sells += 1;
                },
                _ => ()
            }
        }
    }
}
