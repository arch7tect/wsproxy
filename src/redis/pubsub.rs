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
        error!(channel=%channel, error=%e,"Failed to subscribe to Redis");
        return;
    }

    info!(channel=%channel, "Subscribed to Redis");

    let mut stream = pubsub.on_message();

    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                info!(channel=%channel, "Shutdown signal received, unsubscribing");
                drop(stream);
                if let Err(e) = pubsub.unsubscribe(&channel).await {
                    warn!(channel=%channel, error=%e, "Error unsubscribing");
                }
                break;
            }

            msg = stream.next() => {
                if let Some(msg) = msg {
                    let payload = msg.get_payload::<String>().unwrap_or_default();
                    session_addr.do_send(RedisMessage { payload });
                } else {
                    warn!(channel=%channel, "Redis stream ended");
                    break;
                }
            }
        }
    }

    info!(channel=%channel, "Pub/Sub handler stopped");
}