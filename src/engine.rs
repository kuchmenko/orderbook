use std::{collections::HashMap, thread::JoinHandle};

use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::{
    book::{Order, OrderFill, OrderId, OrderIntent},
    id::{EpochSequenceId, EpochSequenceIdGenerator, IdGenerator},
    market::{Market, MarketId},
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Market is not found: id={0}")]
    MarketNotFound(MarketId),
}

#[derive(Debug, PartialEq)]
pub struct CreateMarket {
    pub market_id: MarketId,
    // options?
}

#[derive(Debug, PartialEq)]
pub struct PlaceOrder {
    pub market_id: MarketId,
    pub intent: OrderIntent,
    // options?
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct InputId(pub EpochSequenceId);

#[derive(Debug, PartialEq)]
pub struct Input {
    id: InputId,
    kind: InputKind,
}

#[derive(Debug, PartialEq)]
pub enum InputKind {
    CreateMarket(CreateMarket),
    PlaceOrder(PlaceOrder),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaceOrderOutput {
    market_id: MarketId,
    order: Order,
    fills: Option<Vec<OrderFill>>,
}
#[derive(Debug, Clone, PartialEq)]
pub enum OutputKind {
    PlaceOrder(PlaceOrderOutput),
    CreatedMarket(MarketId),
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct OutputId(EpochSequenceId);

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    id: OutputId,
    input_id: InputId,
    kind: OutputKind,
}

pub struct Engine {
    pub markets: HashMap<MarketId, Market>,

    pub input_ch: mpsc::Receiver<Input>,
    pub output_ch: mpsc::Sender<Output>,

    pub order_ids_generator: EpochSequenceIdGenerator,
    pub output_ids_generator: EpochSequenceIdGenerator,
}

impl Engine {
    pub fn new(
        input: mpsc::Receiver<Input>,
        output: mpsc::Sender<Output>,
        order_ids_generator: EpochSequenceIdGenerator,
        output_ids_generator: EpochSequenceIdGenerator,
    ) -> Self {
        Self {
            markets: HashMap::new(),
            input_ch: input,
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
            info!(?input, "processing incomming input");
            let process_result = match &input.kind {
                InputKind::PlaceOrder(place_order) => self.process_order(place_order),
                InputKind::CreateMarket(create_market) => self.create_market(create_market),
            };

            match process_result {
                Ok(result) => {
                    info!(input_id = ?input.id, ?result, "processed input event");
                    let output = Output {
                        id: OutputId(self.output_ids_generator.next()),
                        input_id: input.id,
                        kind: result,
                    };
                    match self.output_ch.blocking_send(output.clone()) {
                        Ok(_) => {
                            info!(input_id = ?input.id, ?output, "published result")
                        }
                        Err(err) => {
                            error!(input_id = ?input.id, ?output, err = %err, "failed to publish result", );
                        }
                    }
                }
                Err(err) => {
                    error!(input_id = ?input.id, err = %err, "failed to process input event")
                }
            }
        }

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

        Ok(OutputKind::CreatedMarket(market.id))
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

        Ok(OutputKind::PlaceOrder(PlaceOrderOutput {
            market_id,
            order: placement.order,
            fills: placement.fills,
        }))
    }
}

mod tests {

    use tokio::{
        sync::mpsc::{self, error::TryRecvError},
        time::sleep,
    };
    use uuid::Uuid;

    use crate::{
        book::{Order, OrderId, OrderIntent, Side},
        engine::{
            Engine, Input, InputId, InputKind, Output, OutputKind, PlaceOrder, PlaceOrderOutput,
        },
        id::{EpochSequenceId, EpochSequenceIdGenerator},
        market::MarketId,
    };
    fn init_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "debug".into()),
            )
            .with_test_writer()
            .try_init();
    }
    #[tokio::test]
    pub async fn create_engine() {
        init_tracing();
        let (input_tx, mut input_rx) = mpsc::channel::<Input>(1);
        let (output_tx, mut output_rx) = mpsc::channel::<Output>(1);
        let id_generator = EpochSequenceIdGenerator::new();
        let output_id_generator = EpochSequenceIdGenerator::new();

        let mut engine = Engine::new(
            input_rx,
            output_tx.clone(),
            id_generator,
            output_id_generator,
        );
        let market_id = MarketId(Uuid::now_v7());
        let input_id = InputId(EpochSequenceId::new(0, 0));
        let output_id = InputId(EpochSequenceId::new(0, 0));
        let order_id = OrderId(EpochSequenceId::new(0, 0));
        input_tx
            .send(Input {
                id: input_id.clone(),
                kind: InputKind::PlaceOrder(PlaceOrder {
                    market_id: market_id.clone(),
                    intent: OrderIntent {
                        side: Side::Sell,
                        price: 11,
                        amount: 5,
                    },
                }),
            })
            .await
            .expect("failed to post input to engine");

        let handle = engine.spawn();

        let output = output_rx.recv().await;

        drop(input_tx);
        drop(output_tx);

        handle.join().expect("failed to stop engine");
        assert!(
            matches!(
                &output,
                Some(Output {
                    id: output_id,
                    input_id,
                    kind: OutputKind::PlaceOrder(PlaceOrderOutput {
                        market_id,
                        fills: None,
                        order: Order {
                            id: order_id,
                            side: Side::Sell,
                            price: 11,
                            amount: 5,
                        },
                    }),
                }),
            ),
            "unexpected output: {output:#?}"
        );
    }
}
