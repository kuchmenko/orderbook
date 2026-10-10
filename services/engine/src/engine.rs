use std::{collections::HashMap, thread::JoinHandle};

use thiserror::Error;
use tokio::sync::mpsc;

use crate::{
    book::{Order, OrderFill, OrderId, OrderIntent},
    id::{EpochSequenceId, EpochSequenceIdGenerator, IdGenerator},
    market::{Market, MarketId},
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Market is not found: id={_0}")]
    MarketNotFound(MarketId),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct CreateMarket {
    pub market_id: MarketId,
    // options?
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct PlaceOrder {
    pub market_id: MarketId,
    pub intent: OrderIntent,
    // options?
}
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct CancelOrder {
    pub market_id: MarketId,
    pub order_id: OrderId,
    // options?
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub struct InputId(pub EpochSequenceId);

#[derive(Debug, PartialEq, Clone, serde::Deserialize)]
pub struct Input {
    id: InputId,
    kind: InputKind,
}

#[derive(Debug, PartialEq, Clone, serde::Deserialize)]
pub enum InputKind {
    CreateMarket(CreateMarket),
    PlaceOrder(PlaceOrder),
    CancelOrder(CancelOrder),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OrderPlacedOutput {
    pub market_id: MarketId,
    pub order: Order,
    pub fills: Option<Vec<OrderFill>>,
}
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OrderCanceledOutput {
    pub market_id: MarketId,
    pub order: Option<Order>,
}
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MarketCreatedOutput {
    pub market_id: MarketId,
}
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum OutputKind {
    OrderPlaced(OrderPlacedOutput),
    OrderCanceled(OrderCanceledOutput),

    MarketCreated(MarketCreatedOutput),
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, serde::Serialize)]
pub struct OutputId(EpochSequenceId);

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Output {
    pub id: OutputId,
    pub input_id: InputId,
    pub kind: OutputKind,
}

pub struct Engine {
    pub markets: HashMap<MarketId, Market>,

    pub input_ch: mpsc::Receiver<Input>,
    pub input_ack_ch: mpsc::Sender<InputId>,
    pub output_ch: mpsc::Sender<Output>,

    pub order_ids_generator: EpochSequenceIdGenerator,
    pub output_ids_generator: EpochSequenceIdGenerator,
}

impl Engine {
    pub fn new(
        input: mpsc::Receiver<Input>,
        input_ack_ch: mpsc::Sender<InputId>,
        output: mpsc::Sender<Output>,
        order_ids_generator: EpochSequenceIdGenerator,
        output_ids_generator: EpochSequenceIdGenerator,
    ) -> Self {
        Self {
            markets: HashMap::new(),
            input_ch: input,
            input_ack_ch: input_ack_ch,
            output_ch: output,
            order_ids_generator,
            output_ids_generator,
        }
    }

    pub fn spawn(self) -> JoinHandle<Result<(), EngineError>> {
        std::thread::Builder::new()
            .name("matching-engine".to_string())
            .spawn(move || self.run())
            .expect("failed to spawn matching engine")
    }

    pub fn run(mut self) -> Result<(), EngineError> {
        while let Some(input) = self.input_ch.blocking_recv() {
            tracing::info!(?input, "processing incomming input");
            let process_result = match &input.kind {
                InputKind::PlaceOrder(place_order) => self.process_order(place_order),
                InputKind::CancelOrder(cancel_order) => self.cancel_order(cancel_order),
                InputKind::CreateMarket(create_market) => self.create_market(create_market),
            };

            match process_result {
                Ok(result) => {
                    tracing::info!(input_id = ?input.id, ?result, "processed input event");
                    let output = Output {
                        id: OutputId(self.output_ids_generator.next()),
                        input_id: input.id,
                        kind: result,
                    };
                    match self.input_ack_ch.blocking_send(input.id) {
                        Ok(_) => {
                            tracing::info!(input_id = ?input.id, "published input ack")
                        }
                        Err(err) => {
                            tracing::error!(input_id = ?input.id, err = %err, "failed to publish input ack", );
                        }
                    }
                    match self.output_ch.blocking_send(output.clone()) {
                        Ok(_) => {
                            tracing::info!(input_id = ?input.id, ?output, "published result")
                        }
                        Err(err) => {
                            tracing::error!(input_id = ?input.id, ?output, err = %err, "failed to publish result", );
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(input_id = ?input.id, err = %err, "failed to process input event")
                }
            }
        }

        // drop(self.output_ch);

        Ok(())
    }

    pub fn create_market(
        &mut self,
        create_market: &CreateMarket,
    ) -> Result<OutputKind, EngineError> {
        let market = self
            .markets
            .entry(create_market.market_id)
            .or_insert(Market::new(create_market.market_id));

        Ok(OutputKind::MarketCreated(MarketCreatedOutput {
            market_id: market.id,
        }))
    }

    pub fn process_order(&mut self, placement: &PlaceOrder) -> Result<OutputKind, EngineError> {
        let market = self
            .markets
            .entry(placement.market_id)
            .or_insert(Market::new(placement.market_id));
        let market_id = placement.market_id;
        let order = Order {
            id: OrderId(self.order_ids_generator.next()),
            side: placement.intent.side,
            price: placement.intent.price,
            amount: placement.intent.amount,
        };

        let placement = market.book.place(order);

        Ok(OutputKind::OrderPlaced(OrderPlacedOutput {
            market_id,
            order: placement.order,
            fills: placement.fills,
        }))
    }
    pub fn cancel_order(&mut self, cancel_order: &CancelOrder) -> Result<OutputKind, EngineError> {
        let market = self
            .markets
            .entry(cancel_order.market_id)
            .or_insert(Market::new(cancel_order.market_id));
        let market_id = cancel_order.market_id;
        let order = market.book.cancel(&cancel_order.order_id);

        Ok(OutputKind::OrderCanceled(OrderCanceledOutput {
            market_id,
            order,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::{Fill, Side};
    use std::time::Duration;
    use tokio::time::timeout;
    use uuid::Uuid;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn market_id(id: u128) -> MarketId {
        MarketId(Uuid::from_u128(id))
    }

    fn engine() -> Engine {
        let (_, input_rx) = mpsc::channel(1);
        let (input_ack_tx, _) = mpsc::channel(1);
        let (output_tx, _) = mpsc::channel(1);
        Engine::new(
            input_rx,
            input_ack_tx,
            output_tx,
            EpochSequenceIdGenerator::new(),
            EpochSequenceIdGenerator::new(),
        )
    }

    fn placement(market_id: MarketId, side: Side, price: u32, amount: u32) -> PlaceOrder {
        PlaceOrder {
            market_id,
            intent: OrderIntent {
                side,
                price,
                amount,
            },
        }
    }

    fn order(sequence: u64, side: Side, price: u32, amount: u32) -> Order {
        Order {
            id: OrderId(EpochSequenceId::new(0, sequence)),
            side,
            price,
            amount,
        }
    }

    fn place(engine: &mut Engine, placement: PlaceOrder) -> OrderPlacedOutput {
        let OutputKind::OrderPlaced(output) = engine.process_order(&placement).unwrap() else {
            panic!("expected PlaceOrder output");
        };
        output
    }

    fn input(sequence: u64, kind: InputKind) -> Input {
        Input {
            id: InputId(EpochSequenceId::new(9, sequence)),
            kind,
        }
    }

    // Await both the thread join and Engine::run's result, with a deadline so a
    // shutdown regression fails the test rather than blocking it indefinitely.
    async fn join_engine(handle: JoinHandle<Result<(), EngineError>>) {
        timeout(
            TEST_TIMEOUT,
            tokio::task::spawn_blocking(move || handle.join()),
        )
        .await
        .expect("engine did not stop in time")
        .expect("join task panicked")
        .expect("engine thread panicked")
        .expect("engine returned an error");
    }

    #[test]
    fn new_engine_has_no_markets() {
        assert!(engine().markets.is_empty());
    }

    #[test]
    fn creates_distinct_empty_markets() {
        let mut engine = engine();
        for id in [market_id(1), market_id(2)] {
            assert_eq!(
                engine
                    .create_market(&CreateMarket { market_id: id })
                    .unwrap(),
                OutputKind::MarketCreated(MarketCreatedOutput { market_id: id }),
            );
            let market = &engine.markets[&id];
            assert_eq!(market.id, id);
            assert!(market.book.orders.is_empty());
            assert!(market.book.buys.is_empty());
            assert!(market.book.sells.is_empty());
            assert_eq!(market.book.best_buy, 0);
            assert_eq!(market.book.best_sell, u32::MAX);
        }
        assert_eq!(engine.markets.len(), 2);
    }

    #[test]
    fn creating_existing_market_preserves_resting_orders() {
        let mut engine = engine();
        let id = market_id(1);
        engine
            .create_market(&CreateMarket { market_id: id })
            .unwrap();
        let maker = place(&mut engine, placement(id, Side::Sell, 10, 5));

        assert_eq!(
            engine
                .create_market(&CreateMarket { market_id: id })
                .unwrap(),
            OutputKind::MarketCreated(MarketCreatedOutput { market_id: id }),
        );
        assert_eq!(engine.markets.len(), 1);
        assert_eq!(engine.markets[&id].book.orders.len(), 1);
        assert_eq!(
            engine.markets[&id].book.orders[&maker.order.id],
            maker.order
        );
        let taker = place(&mut engine, placement(id, Side::Buy, 10, 5));
        assert_eq!(taker.fills.unwrap()[0].fill.order_id, maker.order.id);
        assert!(engine.markets[&id].book.orders.is_empty());
    }

    #[test]
    fn placing_order_creates_market_and_preserves_intent() {
        for side in [Side::Buy, Side::Sell] {
            let mut engine = engine();
            let id = market_id(1);
            let output = place(&mut engine, placement(id, side, 11, 5));
            let expected = order(1, side, 11, 5);
            assert_eq!(
                output,
                OrderPlacedOutput {
                    market_id: id,
                    order: expected.clone(),
                    fills: None,
                }
            );
            assert_eq!(engine.markets.len(), 1);
            assert_eq!(engine.markets[&id].id, id);
            assert_eq!(engine.markets[&id].book.orders.len(), 1);
            assert_eq!(engine.markets[&id].book.orders[&expected.id], expected);
        }
    }

    #[test]
    fn crossing_orders_in_different_markets_do_not_match() {
        let mut engine = engine();
        let sell = place(&mut engine, placement(market_id(1), Side::Sell, 10, 5));
        let buy = place(&mut engine, placement(market_id(2), Side::Buy, 11, 5));
        assert!(sell.fills.is_none());
        assert!(buy.fills.is_none());
        assert_eq!(engine.markets.len(), 2);
        for output in [sell, buy] {
            let book = &engine.markets[&output.market_id].book;
            assert_eq!(book.orders.len(), 1);
            assert_eq!(book.orders[&output.order.id], output.order);
        }
    }

    #[test]
    fn order_ids_are_sequential_across_markets_and_market_creation() {
        let mut engine = engine();
        for (sequence, id) in [(1, market_id(1)), (2, market_id(2)), (3, market_id(1))] {
            engine
                .create_market(&CreateMarket { market_id: id })
                .unwrap();
            let output = place(&mut engine, placement(id, Side::Buy, 10, 5));
            assert_eq!(output.order.id, OrderId(EpochSequenceId::new(0, sequence)));
        }
    }

    #[test]
    fn order_ids_continue_from_restored_generator() {
        let mut engine = engine();
        engine.order_ids_generator = EpochSequenceIdGenerator::restore(7, 41);
        for sequence in [42, 43] {
            let output = place(&mut engine, placement(market_id(1), Side::Buy, 10, 5));
            assert_eq!(output.order.id, OrderId(EpochSequenceId::new(7, sequence)));
        }
    }

    #[test]
    fn full_match_returns_maker_price_and_removes_resting_order() {
        for (maker_side, taker_side, taker_price) in
            [(Side::Sell, Side::Buy, 11), (Side::Buy, Side::Sell, 9)]
        {
            let mut engine = engine();
            let id = market_id(1);
            place(&mut engine, placement(id, maker_side, 10, 5));
            let output = place(&mut engine, placement(id, taker_side, taker_price, 5));
            assert_eq!(
                output,
                OrderPlacedOutput {
                    market_id: id,
                    order: order(2, taker_side, taker_price, 5),
                    fills: Some(vec![OrderFill {
                        order: order(1, maker_side, 10, 0),
                        fill: Fill {
                            order_id: order(1, maker_side, 10, 5).id,
                            price: 10,
                            total_amount: 5,
                            amount: 5,
                        },
                    }]),
                }
            );
            assert!(engine.markets[&id].book.orders.is_empty());
        }
    }

    #[test]
    fn partial_match_returns_and_preserves_maker_remainder() {
        for (maker_side, taker_side) in [(Side::Sell, Side::Buy), (Side::Buy, Side::Sell)] {
            let mut engine = engine();
            let id = market_id(1);
            place(&mut engine, placement(id, maker_side, 10, 8));
            let output = place(&mut engine, placement(id, taker_side, 10, 3));
            let remaining = order(1, maker_side, 10, 5);
            assert_eq!(
                output,
                OrderPlacedOutput {
                    market_id: id,
                    order: order(2, taker_side, 10, 3),
                    fills: Some(vec![OrderFill {
                        order: remaining.clone(),
                        fill: Fill {
                            order_id: remaining.id,
                            price: 10,
                            total_amount: 8,
                            amount: 3,
                        },
                    }]),
                }
            );
            assert_eq!(engine.markets[&id].book.orders.len(), 1);
            assert_eq!(engine.markets[&id].book.orders[&remaining.id], remaining);
        }
    }

    #[test]
    fn returns_all_fills_and_rests_taker_remainder() {
        let mut engine = engine();
        let id = market_id(1);
        place(&mut engine, placement(id, Side::Sell, 10, 2));
        place(&mut engine, placement(id, Side::Sell, 11, 3));
        let output = place(&mut engine, placement(id, Side::Buy, 12, 7));
        let remainder = order(3, Side::Buy, 12, 2);
        assert_eq!(
            output,
            OrderPlacedOutput {
                market_id: id,
                order: remainder.clone(),
                fills: Some(vec![
                    OrderFill {
                        order: order(1, Side::Sell, 10, 0),
                        fill: Fill {
                            order_id: order(1, Side::Sell, 10, 2).id,
                            price: 10,
                            total_amount: 2,
                            amount: 2,
                        },
                    },
                    OrderFill {
                        order: order(2, Side::Sell, 11, 0),
                        fill: Fill {
                            order_id: order(2, Side::Sell, 11, 3).id,
                            price: 11,
                            total_amount: 3,
                            amount: 3,
                        },
                    },
                ]),
            }
        );
        assert_eq!(engine.markets[&id].book.orders.len(), 1);
        assert_eq!(engine.markets[&id].book.orders[&remainder.id], remainder);
    }

    #[tokio::test]
    async fn processes_buffered_events_in_order_after_input_closes() {
        let id = market_id(1);
        let (input_tx, input_rx) = mpsc::channel(3);
        let (output_tx, mut output_rx) = mpsc::channel(1);
        let engine = Engine::new(
            input_rx,
            output_tx,
            EpochSequenceIdGenerator::restore(3, 20),
            EpochSequenceIdGenerator::restore(4, 30),
        );
        let events = [
            input(8, InputKind::CreateMarket(CreateMarket { market_id: id })),
            input(2, InputKind::PlaceOrder(placement(id, Side::Sell, 10, 5))),
            input(17, InputKind::PlaceOrder(placement(id, Side::Buy, 11, 5))),
        ];
        for event in events {
            input_tx.try_send(event).unwrap();
        }
        drop(input_tx);
        let handle = engine.spawn();
        let expected = [
            Output {
                id: OutputId(EpochSequenceId::new(4, 31)),
                input_id: InputId(EpochSequenceId::new(9, 8)),
                kind: OutputKind::MarketCreated(MarketCreatedOutput { market_id: id }),
            },
            Output {
                id: OutputId(EpochSequenceId::new(4, 32)),
                input_id: InputId(EpochSequenceId::new(9, 2)),
                kind: OutputKind::OrderPlaced(OrderPlacedOutput {
                    market_id: id,
                    order: Order {
                        id: OrderId(EpochSequenceId::new(3, 21)),
                        side: Side::Sell,
                        price: 10,
                        amount: 5,
                    },
                    fills: None,
                }),
            },
            Output {
                id: OutputId(EpochSequenceId::new(4, 33)),
                input_id: InputId(EpochSequenceId::new(9, 17)),
                kind: OutputKind::OrderPlaced(OrderPlacedOutput {
                    market_id: id,
                    order: Order {
                        id: OrderId(EpochSequenceId::new(3, 22)),
                        side: Side::Buy,
                        price: 11,
                        amount: 5,
                    },
                    fills: Some(vec![OrderFill {
                        order: Order {
                            id: OrderId(EpochSequenceId::new(3, 21)),
                            side: Side::Sell,
                            price: 10,
                            amount: 0,
                        },
                        fill: Fill {
                            order_id: OrderId(EpochSequenceId::new(3, 21)),
                            price: 10,
                            total_amount: 5,
                            amount: 5,
                        },
                    }]),
                }),
            },
        ];
        for output in expected {
            assert_eq!(
                timeout(TEST_TIMEOUT, output_rx.recv()).await.unwrap(),
                Some(output)
            );
        }
        assert_eq!(timeout(TEST_TIMEOUT, output_rx.recv()).await.unwrap(), None);
        join_engine(handle).await;
    }

    #[tokio::test]
    async fn live_engine_accepts_events_through_capacity_one_channels() {
        let (input_tx, input_rx) = mpsc::channel(1);
        let (output_tx, mut output_rx) = mpsc::channel(1);
        let handle = Engine::new(
            input_rx,
            output_tx,
            EpochSequenceIdGenerator::new(),
            EpochSequenceIdGenerator::new(),
        )
        .spawn();
        for sequence in 1..=3 {
            let id = market_id(sequence as u128);
            timeout(
                TEST_TIMEOUT,
                input_tx.send(input(
                    sequence,
                    InputKind::CreateMarket(CreateMarket { market_id: id }),
                )),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                timeout(TEST_TIMEOUT, output_rx.recv()).await.unwrap(),
                Some(Output {
                    id: OutputId(EpochSequenceId::new(0, sequence)),
                    input_id: InputId(EpochSequenceId::new(9, sequence)),
                    kind: OutputKind::MarketCreated(MarketCreatedOutput { market_id: id }),
                })
            );
        }
        drop(input_tx);
        assert_eq!(timeout(TEST_TIMEOUT, output_rx.recv()).await.unwrap(), None);
        join_engine(handle).await;
    }

    #[tokio::test]
    async fn closed_empty_input_stops_engine_without_output() {
        let (input_tx, input_rx) = mpsc::channel(1);
        let (output_tx, mut output_rx) = mpsc::channel(1);
        drop(input_tx);
        let handle = Engine::new(
            input_rx,
            output_tx,
            EpochSequenceIdGenerator::new(),
            EpochSequenceIdGenerator::new(),
        )
        .spawn();
        assert_eq!(timeout(TEST_TIMEOUT, output_rx.recv()).await.unwrap(), None);
        join_engine(handle).await;
    }

    fn cancel(engine: &mut Engine, market_id: MarketId, order_id: OrderId) -> OutputKind {
        engine
            .cancel_order(&CancelOrder {
                market_id,
                order_id,
            })
            .unwrap()
    }

    fn canceled(market_id: MarketId, order: Option<Order>) -> OutputKind {
        OutputKind::OrderCanceled(OrderCanceledOutput { market_id, order })
    }

    #[test]
    fn cancel_order_returns_resting_order_and_prevents_future_matching() {
        for (side, opposite) in [(Side::Buy, Side::Sell), (Side::Sell, Side::Buy)] {
            let mut engine = engine();
            let id = market_id(1);
            let maker = place(&mut engine, placement(id, side, 10, 5)).order;
            assert_eq!(cancel(&mut engine, id, maker.id), canceled(id, Some(maker)));
            assert!(engine.markets[&id].book.orders.is_empty());
            let next = place(&mut engine, placement(id, opposite, 10, 5));
            assert!(next.fills.is_none());
            assert_eq!(engine.markets[&id].book.orders.len(), 1);
            assert_eq!(engine.markets[&id].book.orders[&next.order.id], next.order);
        }
    }

    #[test]
    fn cancel_order_returns_only_partially_filled_remainder() {
        for (side, opposite) in [(Side::Buy, Side::Sell), (Side::Sell, Side::Buy)] {
            let mut engine = engine();
            let id = market_id(1);
            let maker = place(&mut engine, placement(id, side, 10, 8)).order;
            place(&mut engine, placement(id, opposite, 10, 3));
            assert_eq!(
                cancel(&mut engine, id, maker.id),
                canceled(id, Some(Order { amount: 5, ..maker })),
            );
            assert!(engine.markets[&id].book.orders.is_empty());
        }
    }

    #[test]
    fn cancel_order_unknown_and_repeated_ids_return_none() {
        let mut engine = engine();
        let id = market_id(1);
        let maker = place(&mut engine, placement(id, Side::Sell, 10, 5)).order;
        assert_eq!(
            cancel(&mut engine, id, order(99, Side::Buy, 1, 1).id),
            canceled(id, None)
        );
        assert_eq!(engine.markets[&id].book.orders.len(), 1);
        assert_eq!(engine.markets[&id].book.orders[&maker.id], maker);
        assert_eq!(
            cancel(&mut engine, id, maker.id),
            canceled(id, Some(maker.clone()))
        );
        assert_eq!(cancel(&mut engine, id, maker.id), canceled(id, None));
        assert!(engine.markets[&id].book.orders.is_empty());
    }

    #[test]
    fn cancel_order_fully_filled_maker_and_taker_return_none() {
        for (side, opposite) in [(Side::Buy, Side::Sell), (Side::Sell, Side::Buy)] {
            let mut engine = engine();
            let id = market_id(1);
            let maker = place(&mut engine, placement(id, side, 10, 5)).order;
            let taker = place(&mut engine, placement(id, opposite, 10, 5)).order;
            for order_id in [maker.id, taker.id] {
                assert_eq!(cancel(&mut engine, id, order_id), canceled(id, None));
            }
            assert!(engine.markets[&id].book.orders.is_empty());
        }
    }

    #[test]
    fn cancel_order_unknown_market_creates_empty_market_without_canceling_elsewhere() {
        let mut engine = engine();
        let original = market_id(1);
        let missing = market_id(2);
        let maker = place(&mut engine, placement(original, Side::Sell, 10, 5)).order;
        assert_eq!(
            cancel(&mut engine, missing, maker.id),
            canceled(missing, None)
        );
        assert_eq!(engine.markets.len(), 2);
        assert_eq!(engine.markets[&missing].id, missing);
        assert!(engine.markets[&missing].book.orders.is_empty());
        assert_eq!(engine.markets[&original].book.orders.len(), 1);
        assert_eq!(engine.markets[&original].book.orders[&maker.id], maker);
    }

    #[test]
    fn cancel_order_affects_only_requested_order_in_requested_market() {
        let mut engine = engine();
        let first = market_id(1);
        let second = market_id(2);
        let target = place(&mut engine, placement(first, Side::Sell, 10, 5)).order;
        let neighbor = place(&mut engine, placement(first, Side::Sell, 10, 3)).order;
        let elsewhere = place(&mut engine, placement(second, Side::Sell, 10, 7)).order;
        assert_eq!(
            cancel(&mut engine, second, target.id),
            canceled(second, None)
        );
        assert_eq!(engine.markets[&first].book.orders.len(), 2);
        assert_eq!(
            cancel(&mut engine, first, target.id),
            canceled(first, Some(target))
        );
        assert_eq!(engine.markets[&first].book.orders.len(), 1);
        assert_eq!(engine.markets[&first].book.orders[&neighbor.id], neighbor);
        assert_eq!(engine.markets[&second].book.orders.len(), 1);
        assert_eq!(
            engine.markets[&second].book.orders[&elsewhere.id],
            elsewhere
        );
        let matched = place(&mut engine, placement(first, Side::Buy, 10, 3));
        assert_eq!(matched.fills.unwrap()[0].fill.order_id, neighbor.id);
        assert!(engine.markets[&first].book.orders.is_empty());
    }

    #[tokio::test]
    async fn cancel_order_inputs_publish_exact_outputs_in_event_order() {
        for (side, opposite) in [(Side::Buy, Side::Sell), (Side::Sell, Side::Buy)] {
            let id = market_id(1);
            let missing = market_id(2);
            let maker = order(1, side, 10, 8);
            let taker = order(2, opposite, 10, 3);
            let remainder = Order {
                amount: 5,
                ..maker.clone()
            };
            let next = order(3, opposite, 10, 8);
            let events = [
                (
                    8,
                    InputKind::PlaceOrder(placement(id, side, 10, 8)),
                    OutputKind::OrderPlaced(OrderPlacedOutput {
                        market_id: id,
                        order: maker.clone(),
                        fills: None,
                    }),
                ),
                (
                    3,
                    InputKind::CancelOrder(CancelOrder {
                        market_id: missing,
                        order_id: maker.id,
                    }),
                    canceled(missing, None),
                ),
                (
                    15,
                    InputKind::CancelOrder(CancelOrder {
                        market_id: id,
                        order_id: order(99, side, 10, 8).id,
                    }),
                    canceled(id, None),
                ),
                (
                    2,
                    InputKind::PlaceOrder(placement(id, opposite, 10, 3)),
                    OutputKind::OrderPlaced(OrderPlacedOutput {
                        market_id: id,
                        order: taker,
                        fills: Some(vec![OrderFill {
                            order: remainder.clone(),
                            fill: Fill {
                                order_id: maker.id,
                                price: 10,
                                total_amount: 8,
                                amount: 3,
                            },
                        }]),
                    }),
                ),
                (
                    42,
                    InputKind::CancelOrder(CancelOrder {
                        market_id: id,
                        order_id: maker.id,
                    }),
                    canceled(id, Some(remainder)),
                ),
                (
                    6,
                    InputKind::CancelOrder(CancelOrder {
                        market_id: id,
                        order_id: maker.id,
                    }),
                    canceled(id, None),
                ),
                (
                    19,
                    InputKind::PlaceOrder(placement(id, opposite, 10, 8)),
                    OutputKind::OrderPlaced(OrderPlacedOutput {
                        market_id: id,
                        order: next.clone(),
                        fills: None,
                    }),
                ),
                (
                    4,
                    InputKind::CancelOrder(CancelOrder {
                        market_id: id,
                        order_id: next.id,
                    }),
                    canceled(id, Some(next)),
                ),
            ];
            let (input_tx, input_rx) = mpsc::channel(events.len());
            let (output_tx, mut output_rx) = mpsc::channel(1);
            let engine = Engine::new(
                input_rx,
                output_tx,
                EpochSequenceIdGenerator::new(),
                EpochSequenceIdGenerator::restore(4, 30),
            );
            let mut expected = Vec::new();
            for (index, (sequence, kind, output)) in events.into_iter().enumerate() {
                input_tx.try_send(input(sequence, kind)).unwrap();
                expected.push(Output {
                    id: OutputId(EpochSequenceId::new(4, 31 + index as u64)),
                    input_id: InputId(EpochSequenceId::new(9, sequence)),
                    kind: output,
                });
            }
            drop(input_tx);
            let handle = engine.spawn();
            for output in expected {
                assert_eq!(
                    timeout(TEST_TIMEOUT, output_rx.recv()).await.unwrap(),
                    Some(output)
                );
            }
            assert_eq!(timeout(TEST_TIMEOUT, output_rx.recv()).await.unwrap(), None);
            join_engine(handle).await;
        }
    }
}
