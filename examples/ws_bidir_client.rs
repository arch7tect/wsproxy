use futures::{SinkExt, StreamExt};
use http::{header::AUTHORIZATION, HeaderValue};
use serde_json::json;
use std::env;
use std::time::Duration;
use tokio::time;
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 4 {
        eprintln!(
            "Usage: cargo run --example ws_bidir_client <agent_id> <session_id> <auth_token>"
        );
        eprintln!(
            "Example: cargo run --example ws_bidir_client agent1 test-session-123 my-secret-token"
        );
        std::process::exit(1);
    }

    let agent_id = &args[1];
    let session_id = &args[2];
    let auth_token = &args[3];

    let url = format!("ws://127.0.0.1:4040/{}/ws/{}", agent_id, session_id);
    println!("Connecting to: {}", url);

    let mut request = url.into_client_request()?;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", auth_token))?,
    );

    let (ws, _resp) = connect_async(request).await?;
    println!("Connected!");
    println!();

    let (mut write, mut read) = ws.split();

    let send_task = tokio::spawn(async move {
        let mut counter = 0;
        loop {
            time::sleep(Duration::from_secs(3)).await;
            counter += 1;

            let message = json!({
                "type": "ping",
                "counter": counter,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });

            println!("Sending upstream: {}", message);

            if let Err(e) = write.send(Message::Text(message.to_string())).await {
                eprintln!("Failed to send message: {}", e);
                break;
            }
        }
    });

    let receive_task = tokio::spawn(async move {
        loop {
            match read.next().await {
                Some(Ok(Message::Text(t))) => {
                    println!("Received downstream: {}", t);
                }
                Some(Ok(Message::Close(f))) => {
                    println!("Closed by server: {:?}", f);
                    break;
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    eprintln!("Read error: {}", e);
                    break;
                }
                None => {
                    println!("Connection closed");
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = send_task => println!("Send task ended"),
        _ = receive_task => println!("Receive task ended"),
    }

    Ok(())
}
