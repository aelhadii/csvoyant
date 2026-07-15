//! Transactional-outbox relay: publishes outbox rows to RabbitMQ, guaranteeing at-least-once
//! delivery of enqueued jobs even if the API crashes between the DB commit and the publish.
//!
//! Each pass claims a batch of unpublished rows with `FOR UPDATE SKIP LOCKED` (so multiple API
//! instances can relay without double-publishing a row), publishes them, and stamps
//! `published_at` in the same transaction. A publish-then-crash re-publishes on the next pass,
//! which is why the worker's job processing is idempotent.

use std::sync::Arc;
use std::time::Duration;

use lapin::Channel;
use sqlx::PgPool;
use tokio::sync::Notify;
use tracing::{error, warn};
use uuid::Uuid;

use crate::jobs::publisher;

/// Dependencies for the relay loop.
pub struct Relay {
    pub pg: PgPool,
    pub channel: Channel,
    /// Nudged by `POST /jobs` so a freshly-written outbox row publishes promptly.
    pub notify: Arc<Notify>,
}

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const BATCH_SIZE: i64 = 50;

/// Run forever: drain the outbox, then wait for a nudge or the poll interval.
pub async fn run(relay: Relay) {
    loop {
        // Drain fully: keep dispatching while batches keep coming back non-empty.
        loop {
            match dispatch_batch(&relay).await {
                Ok(0) => break,
                Ok(_) => continue,
                Err(e) => {
                    error!(error = ?e, "outbox relay batch failed; will retry");
                    break;
                }
            }
        }
        tokio::select! {
            _ = relay.notify.notified() => {}
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
        }
    }
}

/// Publish one batch of unpublished rows. Returns how many were published.
async fn dispatch_batch(relay: &Relay) -> anyhow::Result<usize> {
    let mut tx = relay.pg.begin().await?;

    let rows = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
        "SELECT id, queue, payload FROM outbox \
         WHERE published_at IS NULL \
         ORDER BY created_at \
         LIMIT $1 FOR UPDATE SKIP LOCKED",
    )
    .bind(BATCH_SIZE)
    .fetch_all(&mut *tx)
    .await?;

    if rows.is_empty() {
        return Ok(0);
    }

    let mut published = 0;
    for (id, queue, payload) in &rows {
        let bytes = serde_json::to_vec(payload).expect("outbox payload serializes");
        match publisher::publish_raw(&relay.channel, queue, &bytes).await {
            Ok(()) => {
                sqlx::query("UPDATE outbox SET published_at = now() WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                published += 1;
            }
            Err(e) => {
                // Leave published_at NULL so the row is retried next pass.
                warn!(error = ?e, outbox_id = %id, "failed to publish outbox row; retrying later");
                sqlx::query("UPDATE outbox SET attempts = attempts + 1 WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }

    tx.commit().await?;
    Ok(published)
}
