mod book;
mod engine;
mod id;
mod market;

use std::time::{self, Duration};

use rdkafka::{
    ClientConfig, Message,
    admin::{AdminClient, AdminOptions, NewTopic},
    client::DefaultClientContext,
    consumer::{Consumer, StreamConsumer},
    message::BorrowedMessage,
    producer::{FutureProducer, FutureRecord},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use crate::{
    engine::{Engine, Input, Output},
    id::EpochSequenceIdGenerator,
};

async fn handle_incomming_message<'a>(
    message: &BorrowedMessage<'a>,
    input_tx: mpsc::Sender<Input>,
) -> anyhow::Result<()> {
    let Some(payload) = message.payload() else {
        tracing::info!(?message, "no payload");
        return Ok(());
    };

    match serde_json::from_slice::<Input>(payload) {
        Ok(input) => match input_tx.try_send(input.clone()) {
            Ok(_) => tracing::debug!(?input, "sent input"),
            Err(err) => tracing::error!(?input, %err, "sent input"),
        },
        Err(err) => {
            tracing::error!(?message, ?err, "parse payload");
            return Err(err.into());
        }
    }

    Ok(())
}

async fn run_listener(
    shutdown: CancellationToken,
    input_tx: mpsc::Sender<Input>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", "127.0.0.1:9092")
        .set("group.id", "rust-playground")
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .set("enable.auto.offset.store", "false")
        .create()?;
    consumer.subscribe(&["engine-input"])?;
    println!("We're good with kafka");
    loop {
        tokio::select! {
            message = consumer.recv() => {
                match message {
                    Ok(msg) => {
                        if let Err(err) = handle_incomming_message(&msg, input_tx.clone()).await {
                            tracing::error!(%err, "handle engine input");
                            if let Err(err) = consumer.commit_message(&msg, rdkafka::consumer::CommitMode::Sync) {
                                tracing::error!(%err, "commit message");
                            }
                            break;
                        }
                    },
                    Err(err) => {
                        tracing::error!(%err, "consume engine input");
                        break;
                    },
                }

            },
            _ = shutdown.cancelled() => {
                tracing::info!("Shutting down input listener");
                break;
            },
        };
    }
    println!("dropping input tx");
    drop(input_tx);

    Ok(())
}

async fn run_output_publisher(output_rx: mpsc::Receiver<Output>) -> anyhow::Result<()> {
    let mut output_rx = output_rx;
    let publisher: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", "127.0.0.1:9092")
        .create()?;

    loop {
        tokio::select! {
            output = output_rx.recv() => {
                if output.is_none() {
                    tracing::info!("Output is closed");
                    return Ok(());
                }

                tracing::info!("Output: {:?}", output);
                if let Some(output) = output {
                    let market_id = match &output.kind {
                        engine::OutputKind::OrderPlaced(kind) => Some(kind.market_id),
                        engine::OutputKind::OrderCanceled(kind) => Some(kind.market_id),
                        engine::OutputKind::MarketCreated(kind) => Some(kind.market_id),
                    };

                    if let Some(market_id) = market_id {
                        publisher.send(
                            FutureRecord::to("engine-output")
                                .payload(&serde_json::to_vec(&output)?)
                                .key(&market_id.to_string()),
                            Duration::from_secs(0),
                        ).await.map_err(|(err, _)| anyhow::Error::new(err))?;
                    }

                }

            }
        }
    }
}

async fn ensure_topics() -> anyhow::Result<()> {
    let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", "127.0.0.1:9092")
        .create()?;

    let topics = [
        NewTopic::new(
            "engine-input",
            1,
            rdkafka::admin::TopicReplication::Fixed(1),
        ),
        NewTopic::new(
            "engine-output",
            1,
            rdkafka::admin::TopicReplication::Fixed(1),
        ),
    ];

    let Ok(results) = admin.create_topics(&topics, &AdminOptions::new()).await else {
        anyhow::bail!("Create topics has failed")
    };

    for result in results {
        match result {
            Ok(res) => {
                tracing::info!(%res, "topic was created");
            }
            Err((_, rdkafka::types::RDKafkaErrorCode::TopicAlreadyExists)) => {}
            Err((topic, err)) => {
                tracing::error!(%topic, %err, "topic was not created");
                return Err(anyhow::Error::new(err));
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    // signal
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    ensure_topics().await.unwrap();
    let shutdown = CancellationToken::new();
    let input_listener_shutdown = shutdown.clone();

    let (input_tx, input_rx) = mpsc::channel::<Input>(100);
    let (output_tx, output_rx) = mpsc::channel::<Output>(100);
    let epoch = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .map(|e| e.as_micros() as u32)
        .unwrap();
    let id_generator = EpochSequenceIdGenerator::from_epoch(epoch);
    let output_id_generator = EpochSequenceIdGenerator::from_epoch(epoch);
    let engine = Engine::new(input_rx, output_tx, id_generator, output_id_generator);
    let engine_handle = engine.spawn();
    let input_listener_handler =
        tokio::spawn(async move { run_listener(input_listener_shutdown, input_tx).await });
    let output_publisher_handler =
        tokio::spawn(async move { run_output_publisher(output_rx).await });
    let engine_wait = tokio::task::spawn_blocking(move || engine_handle.join());

    let tasks = async {
        tokio::join!(
            input_listener_handler,
            output_publisher_handler,
            engine_wait,
        )
    };
    tokio::pin!(tasks);

    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            if let Err(err) = result {
                tracing::error!(%err, "listen for CTRL+C");
            }
        tracing::info!("Shutting down system and tasks");
            shutdown.cancel();
            tasks.await;
        tracing::info!("Graceful shutdown is done");
        }
        results = &mut tasks => {
            tracing::info!(?results, "All tasks finished");
        }
    }

    tracing::info!("System offline")
}
