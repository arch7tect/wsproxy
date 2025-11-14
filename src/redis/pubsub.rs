use crate::ws::messages::RedisMessage;
use actix::Addr;
use futures::StreamExt;
use redis::aio::PubSub;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub async fn subscribe_to_channel<A>(
    channel: String,
    session_addr: Addr<A>,
    mut pubsub: PubSub,
    shutdown_token: CancellationToken,
) where
    A: actix::Actor + actix::Handler<RedisMessage>,
    A::Context: actix::dev::ToEnvelope<A, RedisMessage>,
{
    if let Err(e) = pubsub.subscribe(&channel).await {
        error!("Failed to subscribe to Redis channel {}: {}", channel, e);
        return;
    }

    info!("Subscribed to Redis channel: {}", channel);

    let mut stream = pubsub.on_message();

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                info!("Shutdown signal received, unsubscribing from channel: {}", channel);
                drop(stream);
                if let Err(e) = pubsub.unsubscribe(&channel).await {
                    warn!("Error unsubscribing from channel {}: {}", channel, e);
                }
                break;
            }

            msg = stream.next() => {
                if let Some(msg) = msg {
                    let payload = msg.get_payload::<String>().unwrap_or_default();
                    session_addr.do_send(RedisMessage { payload });
                } else {
                    warn!("Redis stream ended for channel: {}", channel);
                    break;
                }
            }
        }
    }

    info!("Pub/Sub handler stopped for channel: {}", channel);
}