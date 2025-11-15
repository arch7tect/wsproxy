use futures::StreamExt;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: cargo run --example redis_subscriber <session_id>");
        eprintln!("Example: cargo run --example redis_subscriber test-session-123");
        std::process::exit(1);
    }

    let session_id = &args[1];
    let redis_url = "redis://127.0.0.1:6379";

    println!("Connecting to Redis: {}", redis_url);
    let client = redis::Client::open(redis_url)?;
    let conn = client.get_async_connection().await?;
    let mut pubsub = conn.into_pubsub();

    let channel = format!("session:{}:up", session_id);
    println!("Subscribing to upstream channel: {}", channel);
    pubsub.subscribe(&channel).await?;

    println!("Listening for upstream messages from WebSocket client...");
    println!();

    let mut stream = pubsub.on_message();
    loop {
        if let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload()?;
            println!("[UPSTREAM] {}", payload);
        }
    }
}
