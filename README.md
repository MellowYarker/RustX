# RustX
A simple text based stock exchange written in Rust

## Setup
Clone the repository, make sure you have Cargo installed. To build and execute the program, type `cargo run`.

## Usage
The instructions will appear when the program starts running, but briefly, there are 3 types of **Requests**: *Order* requests, *Information* requests, and a *Simulation* request.

- **Order requests**: These consist of *buy* and *sell* orders, and have the form `action symbol quantity price`, where symbol is the stock ticker (like `TSLA` for tesla).
- **Info requests**: These consist of basic information requests and have the following format: `action symbol`. The following info requests are currently supported,
  - *Price* request, which returns the latest price at which a trade occured, or helpful messages that inform the user that the market either doesn't exist, or that no trades have occured yet.
  - *Current market view* request, which shows the current buy and sell orders in the market.
  - *History* requests, which shows all the past trades that were filled in the market.
- **Simulation request**: This request lets you simulate random market activity.
  - Format:`simulate symbol num_trades`.
  - There is a 50% chance of buying, 50% chance of selling. The price of each order deviates +/- 5% from the last traded price, and the number of shares is randomly chosen from a short range.

If you don't want to use the interactive version of the program, you can write a simple text file with one request per line, then pass the file as a command line argument `cargo run /path/to/input.txt`.

## Technical Details
### Changelog

#### 4 - Refactoring (Most Recent)
Managed to nearly 2x bandwidth, we're at *1 million orders in < 7 sec* now, but more on that later.

I split the project up into modules to keep my brain from melting, it turned out to be a good call because I was able to do things less so in a Java way and more so in a Rust way; many thanks to [this reddit thread](https://www.reddit.com/r/rust/comments/5ny09j/tips_to_not_fight_the_borrow_checker/dcf6a59/?context=8&depth=9). Modularizing made moving methods from one struct to another less of a hassle, which made the borrow checker happy. It also made me happy, as I was able to [delete redundant HashMap look-ups](https://github.com/MellowYarker/RustX/pull/2/commits/650bc475bfe6f7e021e55ec266aaf135fa9c8fd5#diff-b87f119a5fecd39dd845d0b76bccdec12291ad21571b3a028007e5ebda2fe5bcR220), pass references rather than copies of data as function args, and use `peek` and `peek_mut` rather than `push` and `pop` for the BinaryHeap.

I noticed something while working on this branch though: assuming a bandwidth of 140k orders per second, it takes around `~7.143μs` to execute 1 order. My i5-chip has a clock-rate of 3.4GHz, that is, it can perform `3400` cycles per μs. Roughly speaking, it takes `7.143 μs/order x 3400 cycles/μs = ~24k cycles/order`. This number seemed extremely high to me, since even an L3-cache reference shouldn't take more than a few hundred cycles. This bothered me until this morning, when I realized that I've been compiling in debug mode this entire time. Debug mode doesn't optimize *any part* of the executable (including imported libraries), it also inserts debugging symbols into the executable, so it's basically big and slow. When compiling in *release mode*, 1 million orders were processed in *0.44 sec*, giving a market bandwidth of ~2.72 million orders per second!
#### 3 - Binary Heap
Rather than using vectors which have expensive insertion guarantees, we can use min-max heaps. This way we can still have constant time access to the min/max element, while substantially improving insertion time (thanks Julian!). A million orders takes 13 sec, and this seems to grow linearly with a slope of approximately 1 for the number of orders (i.e *2 million orders = 2 x 1 million = 2 x 13 sec = 26 sec*).
 
Only perceivable downside is since we don't maintain a sorted list of orders, the latency for a user increases if there are a lot of orders on the market. This is a bit annoying, since we print the updated market after a user submits a buy/sell, but could be fixed with some caching/diffing of most relevant orders (order price closest to the last traded price).

So linearly extrapolation suggests our per market order bandwidth is ~*77k orders/sec*, an important detail, since markets are elements of a HashMap. Since these markets don't communicate with each other, I could swap the HashMap for a concurrent HashMap and bump the 77k up to `77k * (# of cores) * (1 - % of time spent acquiring locks)` to get the total number of orders the exchange can handle each second. I would estimate that number is somewhere around *280k* on my quad core desktop, which is pretty good considering the following 2008 quote about the NASDAQ:

>To the extent that the Nasdaq market exists anywhere, it's within a single rack-mounted Dell server in a rented data center somewhere across the Hudson River. That machine routinely processes 70,000 orders, cancelations and trades per second but can handle up to 250,000 per second--enough to deal with trades on the Nasdaq plus the London and Paris stock exchanges with room to spare. - [Forbes](https://www.forbes.com/forbes/2009/0112/056.html?sh=66da69317cc7)

Although to be fair, we don't handle cancellations or or even user accounts, but it's not so bad for an introductory Rust project :)
#### 2 - Sort Vec Ascending and Descending
By sorting the sell orders in descending order, we can pop the lowest offer off the back of the vector instead of removing from the front. This brought the runtime down to ~23 sec for 1 million orders. We still move a lot of data when inserting in a way that maintains order, so there are still gains up for grabs if we use a data structure that has better insertion runtime guarantees.

Below is a flamegraph showing the time spent in each function call (kind of like a graphical version of GProf and Perf). Clearly, a large portion (~60%) of the execution time is spent in `Vec::insert`, so if I do any more work on this, it will be modifying the data structure that market orders are stored in.
![Flamegraph](./media/performance.png)

#### 1 - Baseline
Basically no effort has gone to performance, but I measured about 1.5 min for 1 million orders (no print statements). I suspect most of the runtime is spent moving elements in the buy/sell vectors, and that using BSTs here would result in far better performance (we insert/remove the front of a large vector *very frequently*).

## Demo
In this demo, 3 `buy` orders are placed and subsequently sorted by price, then 4 `sell` orders are placed. Some of these `sell` orders consume existing buy orders and then disappear, others consume buy orders and then remain on the market.

![Demo gif](./media/edit-exchange.gif)
