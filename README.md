# RustX
A simple text based stock exchange written in Rust

## Setup
Clone the repository, make sure you have Cargo installed. To build and execute the program, type `cargo run`.

## Usage
The instructions will appear when the program starts running, but briefly, there are 2 types of **Requests**: *Order* requests and *Information* requests.

- **Order requests**: These consist of *buy* and *sell* orders, and have the form `action symbol quantity price`, where symbol is the stock ticker (like `TSLA` for tesla).
- **Info requests**: These consist of basic information requests and have the following format: `action symbol`. The following info requests are currently supported,
  - *Price* request, which returns the latest price at which a trade occured, or helpful messages that informs the user that the market either doesn't exist, or that no trades have occured yet.
  - *Current market view* request, which shows the current buy and sell orders in the market as tables.
  - *History* requests, which shows all the past trades that were filled in the market.
