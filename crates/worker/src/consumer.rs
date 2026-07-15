//! RabbitMQ consume loop: run each job, ack on success, retry-with-backoff on a retryable
//! failure (up to MAX_ATTEMPTS), and dead-letter on a permanent failure or exhausted retries.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use lapin::message::Delivery;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions, BasicQosOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel};
use shared::{INGESTION_QUEUE, JobMessage, MAX_ATTEMPTS};
use tracing::{Instrument, error, info, info_span, warn};

use crate::ingest::{self, Context};
use crate::repo;

/// Start consuming and process deliveries concurrently (bounded by the prefetch QoS).
pub async fn run(channel: Channel, ctx: Arc<Context>) -> anyhow::Result<()> {
    channel.basic_qos(8, BasicQosOptions::default()).await?;
    let mut consumer = channel
        .basic_consume(
            INGESTION_QUEUE,
            "csvoyant-worker",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;
    info!(queue = INGESTION_QUEUE, "worker consuming");

    while let Some(delivery) = consumer.next().await {
        match delivery {
            Ok(delivery) => {
                let channel = channel.clone();
                let ctx = ctx.clone();
                tokio::spawn(async move { handle(&channel, &ctx, delivery).await });
            }
            Err(e) => error!(error = ?e, "error receiving delivery"),
        }
    }
    Ok(())
}

async fn handle(channel: &Channel, ctx: &Context, delivery: Delivery) {
    let message: JobMessage = match serde_json::from_slice(&delivery.data) {
        Ok(m) => m,
        Err(e) => {
            // Unparseable message: nothing we can do with it — drop it (ack) rather than loop.
            warn!(error = ?e, "discarding unparseable job message");
            let _ = delivery.ack(BasicAckOptions::default()).await;
            return;
        }
    };

    let span = info_span!(
        "ingest_job",
        job_id = %message.job_id,
        user_id = %message.user_id,
        attempt = message.attempt,
    );

    match ingest::run_job(ctx, &message).instrument(span).await {
        Ok(()) => {
            let _ = delivery.ack(BasicAckOptions::default()).await;
        }
        // Retryable and attempts remain: back off and republish a new attempt.
        Err(e) if e.is_retryable() && message.attempt + 1 < MAX_ATTEMPTS => {
            let _ = repo::increment_attempts(&ctx.pg, message.job_id).await;
            let backoff = Duration::from_secs(2u64.pow(message.attempt.min(6)));
            warn!(
                job_id = %message.job_id,
                attempt = message.attempt,
                backoff_secs = backoff.as_secs(),
                reason = e.message(),
                "retryable failure; scheduling retry",
            );
            tokio::time::sleep(backoff).await;

            let retry = JobMessage {
                attempt: message.attempt + 1,
                ..message.clone()
            };
            match republish(channel, &retry).await {
                Ok(()) => {
                    let _ = delivery.ack(BasicAckOptions::default()).await;
                }
                Err(pub_err) => {
                    // Couldn't requeue — fail the job and dead-letter the message.
                    error!(error = ?pub_err, job_id = %message.job_id, "failed to republish retry");
                    let _ = repo::mark_failed(&ctx.pg, message.job_id, e.message()).await;
                    dead_letter(&delivery).await;
                }
            }
        }
        // Permanent, or retries exhausted: record the reason and dead-letter for inspection.
        Err(e) => {
            warn!(
                job_id = %message.job_id,
                attempt = message.attempt,
                retryable = e.is_retryable(),
                reason = e.message(),
                "job failed",
            );
            let _ = repo::mark_failed(&ctx.pg, message.job_id, e.message()).await;
            dead_letter(&delivery).await;
        }
    }
}

/// Nack without requeue → the queue's `x-dead-letter-exchange` routes it to the DLQ.
async fn dead_letter(delivery: &Delivery) {
    let _ = delivery
        .nack(BasicNackOptions {
            requeue: false,
            multiple: false,
        })
        .await;
}

async fn republish(channel: &Channel, message: &JobMessage) -> Result<(), lapin::Error> {
    let payload = serde_json::to_vec(message).expect("JobMessage serializes");
    channel
        .basic_publish(
            "",
            INGESTION_QUEUE,
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default().with_delivery_mode(2),
        )
        .await?
        .await?;
    Ok(())
}
