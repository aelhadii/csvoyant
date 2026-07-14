//! Durable RabbitMQ topology for the ingestion pipeline.
//!
//! - `ingestion.jobs` — the main durable work queue, dead-lettered to the DLX on rejection.
//! - `ingestion.dlx` — the dead-letter exchange (fanout).
//! - `ingestion.jobs.dead` — the dead-letter queue bound to the DLX, where failed/retried
//!   messages land for inspection and backoff retry (Prompt C wires the retry logic).

use lapin::options::{ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::{AMQPValue, FieldTable};
use lapin::{Channel, ExchangeKind};
use shared::{DEAD_LETTER_EXCHANGE, DEAD_LETTER_QUEUE, INGESTION_QUEUE};

pub async fn declare(channel: &Channel) -> anyhow::Result<()> {
    let durable = QueueDeclareOptions {
        durable: true,
        ..Default::default()
    };

    // Dead-letter exchange (fanout) + its queue.
    channel
        .exchange_declare(
            DEAD_LETTER_EXCHANGE,
            ExchangeKind::Fanout,
            ExchangeDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;
    channel
        .queue_declare(DEAD_LETTER_QUEUE, durable, FieldTable::default())
        .await?;
    channel
        .queue_bind(
            DEAD_LETTER_QUEUE,
            DEAD_LETTER_EXCHANGE,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    // Main jobs queue, configured to dead-letter to the DLX.
    let mut args = FieldTable::default();
    args.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(DEAD_LETTER_EXCHANGE.into()),
    );
    channel
        .queue_declare(INGESTION_QUEUE, durable, args)
        .await?;

    Ok(())
}
