use futures::{StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = "ws://127.0.0.1:4040/agent1/ws/test-session-123";
    let (mut ws, _resp) = connect_async(url).await?;

    loop {
        match ws.next().await {
            Some(Ok(Message::Text(t))) => {
                println!("Received: {t}");
            }
            Some(Ok(Message::Close(f))) => {
                println!("Closed by server: {:?}", f);
                break;
            }
            Some(Ok(_)) => {}           // Binary / Pong — ignore
            Some(Err(e)) => {            // read error → done
                eprintln!("Read error: {e}");
                break;
            }
            None => break,               // stream ended
        }
    }
    Ok(())
}
