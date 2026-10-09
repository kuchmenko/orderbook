mod book;
mod engine;
mod id;
mod market;

use rdkafka::{
    ClientConfig, Message,
    consumer::{Consumer, StreamConsumer},
    message::BorrowedMessage,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use crate::{
    engine::{Engine, Input, Output},
    id::EpochSequenceIdGenerator,
};

async fn handle_incomming_message<'a>(
    message: BorrowedMessage<'a>,
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
            tracing::error!(?message, %err, "parse payload");
            return Ok(());
        }
    }

    Ok(())
}

async fn run_listener(
    shutdown: CancellationToken,
    input_tx: mpsc::Sender<Input>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", "localhost:9092")
        .set("group.id", "rust-playground")
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .set("enable.auto.offset.store", "false")
        .create()?;
    consumer.subscribe(&["engine-input"]);
    loop {
        tokio::select! {
            message = consumer.recv() => {
                match message {
                    Ok(msg) => handle_incomming_message(msg, input_tx.clone()),
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
    drop(input_tx);

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
    let shutdown = CancellationToken::new();
    let input_listener_shutdown = shutdown.clone();

    let (input_tx, input_rx) = mpsc::channel::<Input>(100);
    let (output_tx, mut output_rx) = mpsc::channel::<Output>(100);
    let id_generator = EpochSequenceIdGenerator::new();
    let output_id_generator = EpochSequenceIdGenerator::new();
    let engine = Engine::new(input_rx, output_tx, id_generator, output_id_generator);

    let engine_handle = engine.spawn();
    let input_listener_handler =
        tokio::spawn(async move { run_listener(input_listener_shutdown, input_tx).await });
    let output_publisher_handler = tokio::spawn(async move {
        loop {
            tokio::select! {
                output = output_rx.recv() => {
                    if output.is_none() {
                        tracing::info!("Output is closed");
                        return;
                    }

                    tracing::info!("Output: {:?}", output);
                }
            }
        }
    });
    let engine_wait = tokio::task::spawn_blocking(move || engine_handle.join());

    tokio::signal::ctrl_c().await.unwrap();
    shutdown.cancel();
    let _ = tokio::join!(
        input_listener_handler,
        output_publisher_handler,
        engine_wait,
    );

    tracing::info!("System offline")
}
