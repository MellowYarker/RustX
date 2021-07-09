# RustX
A fast, terminal oriented stock exchange written in Rust.

## Setup
You'll need the following software installed on your computer:
- Cargo
- Postgresql
### Setting up the Postgres Server
If you're setting postgres up yourself, follow the script [here](https://www.postgresql.org/docs/current/install-short.html). I will have this autoconfigure at some point in the future, this is just a reference for now.
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
### Changelog
#### 6 - Persistence, or, the story of compromises (consistency vs. latency).
I will summarize the last ~month of work here shortly.
#### 5 - Adding Accounts (Most Recent)
The branch **WHY** (as in, *why am I still working on this*), in which I implemented and integrated accounts everywhere, has significantly changed the structure of the program. In fact, the previous performance numbers are more or less irrelevant now, since our simulation has been rewritten.

**Before WHY**:
We had one account buy shares (in one market) and another account sell shares (in the same market). These two accounts traded with each other *n* times, where *n* was large.
This was no longer practical after integrating user accounts, since each time a user submits an order, we have to compare that order with all the other orders that user placed in the market to ensure they won't fulfill one of their own pending orders. This is computationally expensive when a user has a lot of orders in one pending market, and I realized that I could either make more program much more complicated, or recognize that the simulation wasn't very realistic.

**New Simulation**:
We have many accounts both buying and selling in many markets.

Because of this change, we can no longer measure performance in the way we did before, as we now have two independent variables (excluding values determined randomly during sim runtime like price, order type, which account will place an order, the market to operate in, etc).

#### 4 - Refactoring
Managed to nearly 2x bandwidth, we're at *1 million orders in < 7 sec* now, but more on that later.

I split the project up into modules to keep my brain from melting, it turned out to be a good call because I was able to do things less so in a Java way and more so in a Rust way; many thanks to [this reddit thread](https://www.reddit.com/r/rust/comments/5ny09j/tips_to_not_fight_the_borrow_checker/dcf6a59/?context=8&depth=9). Modularizing made moving methods from one struct to another less of a hassle, which made the borrow checker happy. It also made me happy, as I was able to [delete redundant HashMap look-ups](https://github.com/MellowYarker/RustX/pull/2/commits/650bc475bfe6f7e021e55ec266aaf135fa9c8fd5#diff-b87f119a5fecd39dd845d0b76bccdec12291ad21571b3a028007e5ebda2fe5bcR220), pass references rather than copies of data as function args, and use `peek` and `peek_mut` rather than `push` and `pop` for the BinaryHeap.

I noticed something while working on this branch though: assuming a bandwidth of 140k orders per second, it takes around `~7.143μs` to execute 1 order. My i5-chip has a clock-rate of 3.4GHz, that is, it can perform `3400` cycles per μs. Roughly speaking, it takes `7.143 μs/order x 3400 cycles/μs = ~24k cycles/order`. This number seemed extremely high to me, since even an L3-cache reference shouldn't take more than a few hundred cycles. Eventually I realized that I've been compiling in debug mode this entire time. Debug mode doesn't optimize *any part* of the executable (including imported libraries), it also inserts debugging symbols into the executable, making it big and slow. When compiling in *release mode*, 1 million orders were processed in *0.44 sec*, giving a market bandwidth of ~2.72 million orders per second!
#### 3 - Binary Heap
Rather than using vectors which have expensive insertion guarantees, we can use min-max heaps. This way we can still have constant time access to the min/max element, while substantially improving insertion time (thanks Julian!). A million orders takes 13 sec, and this seems to grow linearly with a slope of approximately 1 for the number of orders (i.e *2 million orders = 2 x 1 million = 2 x 13 sec = 26 sec*).
 
Only perceivable downside is since we don't maintain a sorted list of orders, the latency for a user increases if there are a lot of orders on the market. This is a bit annoying, since we print the updated market after a user submits a buy/sell, but could be fixed with some caching/diffing of most relevant orders (order price closest to the last traded price).

So linear extrapolation suggests our per market order bandwidth is ~*77k orders/sec*, an important detail, since markets are elements of a HashMap. Since these markets don't communicate with each other, I could swap the HashMap for a concurrent HashMap and bump the 77k up to `77k * (# of cores) * (1 - % of time spent acquiring locks)` to get the total number of orders the exchange can handle each second. I would estimate that number is somewhere around *280k* on my quad core desktop, which is pretty good considering the following 2008 quote about the NASDAQ:

>To the extent that the Nasdaq market exists anywhere, it's within a single rack-mounted Dell server in a rented data center somewhere across the Hudson River. That machine routinely processes 70,000 orders, cancelations and trades per second but can handle up to 250,000 per second--enough to deal with trades on the Nasdaq plus the London and Paris stock exchanges with room to spare. - [Forbes](https://www.forbes.com/forbes/2009/0112/056.html?sh=66da69317cc7)

Although to be fair, we don't handle cancellations or or even user accounts, but it's not so bad for an introductory Rust project :)
#### 2 - Sort Vec Ascending and Descending
By sorting the sell orders in descending order, we can pop the lowest offer off the back of the vector instead of removing from the front. This brought the runtime down to ~23 sec for 1 million orders. We still move a lot of data when inserting in a way that maintains order, so there are still gains up for grabs if we use a data structure that has better insertion runtime guarantees.

Below is a flamegraph showing the time spent in each function call (kind of like a graphical version of GProf and Perf). Clearly, a large portion (~60%) of the execution time is spent in `Vec::insert`, so if I do any more work on this, it will be modifying the data structure that market orders are stored in.
![Flamegraph](./media/performance.png)

#### 1 - Baseline
Basically no effort has gone to performance, but I measured about 1.5 min for 1 million orders (no print statements). I suspect most of the runtime is spent moving elements in the buy/sell vectors, and that using BSTs here would result in far better performance (we insert/remove the front of a large vector *very frequently*).
