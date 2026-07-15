//! Publishing messages onto RabbitMQ.

use lapin::options::BasicPublishOptions;
use lapin::{BasicProperties, Channel};

/// Publish a persistent message to a queue via the default exchange (routing key = queue name).
pub async fn publish_raw(
    channel: &Channel,
    queue: &str,
    payload: &[u8],
) -> Result<(), lapin::Error> {
    channel
        .basic_publish(
            "",
            queue,
            BasicPublishOptions::default(),
            payload,
            BasicProperties::default()
                .with_delivery_mode(2) // persistent
                .with_content_type("application/json".into()),
        )
        .await?
        .await?;
    Ok(())
}
