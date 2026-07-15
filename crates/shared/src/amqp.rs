//! AMQP topology and the ingestion job message, shared by the API (publisher) and worker
//! (consumer) so both agree on the queue names and payload shape.

use lapin::options::{ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions};
use lapin::types::{AMQPValue, FieldTable};
use lapin::{Channel, ExchangeKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DEAD_LETTER_EXCHANGE, DEAD_LETTER_QUEUE, INGESTION_QUEUE};

/// The payload published for each ingestion job. `attempt` starts at 0 and increments on retry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobMessage {
    pub job_id: Uuid,
    pub user_id: Uuid,
    pub source_url: String,
    #[serde(default)]
    pub attempt: u32,
}

/// Declare the durable ingestion topology (idempotent): the work queue (dead-lettered to the
/// DLX) plus the dead-letter exchange and its queue. Safe to call from every service on startup.
pub async fn declare_topology(channel: &Channel) -> Result<(), lapin::Error> {
    let durable = QueueDeclareOptions {
        durable: true,
        ..Default::default()
    };

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
