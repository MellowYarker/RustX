// TODO: We want several data structures, maybe inside one "Buffer" struct
//       where we can store:
//       1. Orders as follows
//           {
//             action: Option<String>,
//             symbol: Option<String>,
//             quantity: Option<i32>,
//             filled: Option<i32>,
//             price: Option<f64>,
//             order_id: Option<i32>,
//             status: Option<OrderStatus>,
//             user_id: Option<i32>
//           }
//
//          By storing orders in this way, we can represent a NEW order (i.e one the database has
//          never seen) by having some fields like action or symbol be Some(string). Meanwhile, an
//          old order would have action, symbol, quantity, price, order_id, user_id all set to None
//          in our Buffer structure, since the database knows the values already and they do not
//          change.
//
//          Therefore, orders the DB knows about will be stored in the buffer as:
//           {
//             action: None,
//             symbol: None,
//             quantity: None,
//             filled: Option<i32>, // Some(i32) if update occured, else None
//             price: None,
//             order_id: None,
//             status: Option<OrderStatus>, // Some(OrderStatus) if update occured, else None
//             user_id: None
//           }
//          This makes it easy for us to determine
//          a. If we need to to an insert (new order) or update (old order).
//          b. Which fields need to be updated for an old order.
//          c. We can differentiate between a complete order, a cancelled order, and a pending
//          order.
//
//      2. Trades can just be stored in a vector. They are always new and unique!
//      3. New accounts can probably still be inserted immediately
//      4. The "exchangeStats", i.e max order ID, can just be read straight from the exchange
//
//
//  Ideally, we will spin up some thread whenever we want to perform DB writes so this doesn't
//  affect program operation. This means we want to move the buffers to the other thread,
//  essentially draining them in the main thread (not deallocating!).
//
//  Somehow, we should run this all in a loop, i.e every n seconds, write the buffer to the DB. Or
//  instead, once the buffer reaches a certain size (10MB?), write the contents to the DB. It
//  really depends on:
//
//      1. Overall program memory consumption
//          - If the program is running hot, we will have to decrease the buffer capacity.
//            However, we can probably control a few things like # users in the cache (evict LRU),
//            # of pending orders we want to store in each market, etc.
//      2. Latency of writing to the database.
//      3. Ability to write to DB by moving buffer to another thread and continuing normal
//         operations (in this case, we could have a very large buffer).
