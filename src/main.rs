mod book;
mod engine;
mod id;
mod market;

use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use crate::{
    engine::{Engine, Input, Output},
    id::EpochSequenceIdGenerator,
};

fn main() {
    // signal
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let (input_tx, mut input_rx) = mpsc::channel::<Input>(1);
    let (output_tx, mut output_rx) = mpsc::channel::<Output>(1);
    let id_generator = EpochSequenceIdGenerator::new();
    let output_id_generator = EpochSequenceIdGenerator::new();
    let engine = Engine::new(
        input_rx,
        output_tx.clone(),
        id_generator,
        output_id_generator,
    );

    let handle = engine.spawn();
    handle.join().expect("failed to stop engine");

    // let mut places = vec![];

    // // market.sell(5, 11);
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 11,
    //     amount: 5,
    // });
    //
    // // market.sell(13, 10);
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 10,
    //     amount: 13,
    // });
    // // market.sell(2, 7);
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 7,
    //     amount: 2,
    // });
    // // market.sell(10, 6);
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 6,
    //     amount: 10,
    // });
    // //
    // //
    // println!("market {}", market);
    // //
    // // market.buy(13, 9);
    // market.place(OrderIntent {
    //     side: Side::Buy,
    //     price: 9,
    //     amount: 13,
    // });
    // // market.buy(53, 4);
    // market.place(OrderIntent {
    //     side: Side::Buy,
    //     price: 4,
    //     amount: 53,
    // });
    // // market.buy(100, 2);
    // market.place(OrderIntent {
    //     side: Side::Buy,
    //     price: 2,
    //     amount: 100,
    // });
    // //
    // println!("market {}", market);
    // // // market.sell(63, 3);
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 3,
    //     amount: 63,
    // }); // rest 9 at price 3
    // println!("market {}", market);
    // // // // // market.buy(9, 10);
    // market.place(OrderIntent {
    //     side: Side::Buy,
    //     price: 10,
    //     amount: 10,
    // }); //rest 1 at price 10
    // println!("market {}", market);
    // // market.buy(12, 9);
    // market.place(OrderIntent {
    //     side: Side::Buy,
    //     price: 9,
    //     amount: 12,
    // }); // rest 12 at price 9
    // println!("market {}", market);
    // // // // // // market.sell(12, 5);
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 5,
    //     amount: 12,
    // }); // no rest, leave  1 at 9
    // println!("market {}", market);
    // // // // // market.sell(100, 1);
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 1,
    //     amount: 100,
    // }); // buy 1 at 9, no rest
    // println!("market {}", market);
    // // // market.cancel(&first.order.id);
    // // // println!("market {}", market);
    // // //
    // // // market.cancel(&second.order.id);
    // // // println!("market {}", market);
    // //
    // // market.sell(12, 3); // 12 at 3
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 3,
    //     amount: 12,
    // });
    // // market.sell(15, 5); // 15 at 5
    // market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 5,
    //     amount: 15,
    // });
    // //
    // println!("market {}", market);
    // //
    // // market.buy(100, 11); //  10 buy at 11
    // market.place(OrderIntent {
    //     side: Side::Buy,
    //     price: 11,
    //     amount: 100,
    // });
    // println!("market {}", market);
    // // // market.buy(352, 1); //  352 buy at 1
    // market.place(OrderIntent {
    //     side: Side::Buy,
    //     price: 1,
    //     amount: 352,
    // });
    // //
    // println!("market {}", market);
    // // //
    // // // market.sell(173, 5); // 8 - 15 = -7 = 7 sell at 5
    // let toCancel = market.place(OrderIntent {
    //     side: Side::Sell,
    //     price: 5,
    //     amount: 173,
    // });
    // //
    // println!("market 173 {}", market);
    //
    // market.cancel(&toCancel.order.id);
    // println!("market 173 {}", market);
    // // //
    // // // market.buy(400, 15); //  352 buy at 1
    // // market.place(PlaceIntent {
    // //     side: Side::Buy,
    // //     price: 15,
    // //     amount: 400,
    // // });
    // // println!("market {}", market);
    println!("Hello, world!");
}
