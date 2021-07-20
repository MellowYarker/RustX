# RustX
A fast, terminal oriented stock exchange written in Rust.

You can read about the technical details near the bottom of the readme, and there will be an `architecture.md` at some point.

## Setup
You'll need the following software installed on your computer:
- Cargo
- Postgresql
- Redis

I will make a script to set this all up at some point, but for now you have to do it manually, sry my bad.
### Setting up the Redis Server
Once Redis has been installed, if you're on linux, just run `sudo systemctl start redis`. 

### Setting up the Postgres Server
If you're setting postgres up yourself, follow the script [here](https://www.postgresql.org/docs/current/install-short.html).
```
./configure
make
su
make install
adduser postgres
mkdir /usr/local/pgsql/data
chown postgres /usr/local/pgsql/data
su - postgres
/usr/local/pgsql/bin/initdb -D /usr/local/pgsql/data
/usr/local/pgsql/bin/pg_ctl -D /usr/local/pgsql/data -l logfile start
/usr/local/pgsql/bin/createdb rustx
/usr/local/pgsql/bin/psql rustx
```
The postgres server might not be running after you run `pg_ctl`. In this case, check `src/database/setup.txt` and replace the commands with whatever command line tools your OS offers.

Next, we want to create the tables, which can be found in `/src/database/schema.sql`.
`psql database_name -U postgres < src/database/schema.sql` should do the trick, if not, run psql with the `-s` flag, and then import the file with `\i /path/to/file` from within the postgres client.

The final step is to populate the `Markets` table with whatever markets you plan on hosting on your exchange. Obviously, you can insert this directly in the database however you like, but you have the option of running the program and updating the DB as an Admin user. For example, the following file `src/database/NYSE.csv` has all the NYSE stock info for a certain moment in time. The function in question, `upgrade_db`, only cares about the company name and stock ticker.
To upload this data to postgres, simply:
1. `cargo run`
2. Create an admin account like so: `account create admin password`
3. Request the DB upgrade: `upgrade_db rustx admin password`, you will be prompted for the input file, so provide the path.
4. The program is ready for use!

### Running the program
To build and execute the program in interactive mode, type `cargo run --release`.

If you don't want to use the interactive version of the program, you can write a simple text file with one request per line, then pass the file as a command line argument `cargo run --release /path/to/input.txt`.

If you don't want to recompile each time you run the program, use `cargo build --release` instead. The executable can be found under `/target/release/exchange`, so if you want to pass an input file, just enter it as a command line argument again.

## Usage
The instructions will appear when the program starts running, but briefly, there are 5 types of **Requests**: *Order* requests, *Cancel* request, *Information* requests, a *Simulation* request, and *Account* requests.

- **Order requests**: These consist of *buy* and *sell* orders, and have the form `action symbol quantity price username password`, where symbol is the stock ticker (like `TSLA` for tesla).
- **Cancel request**: This request allows a user to cancel an order that they had previously placed. It looks like: `cancel symbol order_id username password`.
  - Note that like in a real exchange, a user can only cancel the non-filled portion of the order.
- **Info requests**: These consist of basic information requests and have the following format: `<request> symbol`. The following info requests are currently supported,
  - *Price* request, which returns the latest price at which a trade occured, or helpful messages that inform the user that the market either doesn't exist, or that no trades have occured yet.
  - *Current market view* request, which shows the most relevant buy and sell orders in the market.
  - *History* request, which shows all the past trades that were executed in the market.
- **Simulation request**: This request lets you simulate random market activity.
  - Format:`simulate num_users num_markets num_orders`.
  - There is a 50% chance of buying, 50% chance of selling. The price of each order deviates +/- 5% from the last traded price, and the number of shares is randomly chosen from a short range. This simulation format lets us test likely exchange activity that could occur in the real world.
- **Account requests**: These requests allow you to create a new user or see the activity of a user (*authentication required*).


## Demo [outdated]
- This demo is no longer an accurate representation of the program. The reason I haven't updated the demo is because the frontend has gotten a bit chaotic in terms of formatting, so a new demo will be uploaded once a web frontend has been made.

In this demo, 3 `buy` orders are placed and subsequently sorted by price, then 4 `sell` orders are placed. Some of these `sell` orders consume existing buy orders and then disappear, others consume buy orders and then remain on the market.

![Demo gif](./media/edit-exchange.gif)


## Technical Details
See the [change log](https://github.com/MellowYarker/RustX/wiki/Change-Log) in the Wiki for a timeline/blog type vibe
