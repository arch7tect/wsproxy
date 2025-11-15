use futures::{SinkExt, StreamExt};
use http::{header::AUTHORIZATION, HeaderValue};
use redis::AsyncCommands;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message};

async fn setup_redis() -> redis::Client {
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    redis::Client::open(redis_url).expect("Failed to create Redis client")
}

async fn set_auth_token(session_id: &str, token: &str) {
    let client = setup_redis().await;
    let mut conn = client.get_async_connection().await.unwrap();
    let key = format!("session:{}:auth", session_id);
    let _: () = conn.set(&key, token).await.unwrap();
}

async fn clear_auth_token(session_id: &str) {
    let client = setup_redis().await;
    let mut conn = client.get_async_connection().await.unwrap();
    let key = format!("session:{}:auth", session_id);
    let _: () = conn.del(&key).await.unwrap_or(());
}

#[tokio::test]
async fn test_websocket_connection_without_auth() {
    let url = "ws://127.0.0.1:4040/test-agent/ws/test-no-auth";
    let result = connect_async(url).await;

    assert!(result.is_err() || {
        if let Ok((_, resp)) = result {
            resp.status() == 401 || resp.status() == 403
        } else {
            false
        }
    });
}

#[tokio::test]
async fn test_websocket_connection_with_invalid_token() {
    let session_id = "test-invalid-ws";
    let url = format!("ws://127.0.0.1:4040/test-agent/ws/{}", session_id);

    set_auth_token(session_id, "correct-token").await;

    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str("Bearer wrong-token").unwrap(),
    );

    let result = connect_async(request).await;

    assert!(result.is_err() || {
        if let Ok((_, resp)) = result {
            resp.status() == 403
        } else {
            false
        }
    });

    clear_auth_token(session_id).await;
}

#[tokio::test]
async fn test_websocket_connection_with_valid_token() {
    let session_id = "test-valid-ws";
    let token = "valid-token-456";
    let url = format!("ws://127.0.0.1:4040/test-agent/ws/{}", session_id);

    set_auth_token(session_id, token).await;

    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );

    let result = connect_async(request).await;

    assert!(result.is_ok(), "WebSocket connection should succeed with valid token");

    if let Ok((mut ws, _)) = result {
        ws.close(None).await.ok();
    }
}

#[tokio::test]
async fn test_downstream_message_delivery() {
    let session_id = "test-downstream";
    let token = "downstream-token";
    let url = format!("ws://127.0.0.1:4040/test-agent/ws/{}", session_id);

    set_auth_token(session_id, token).await;

    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );

    let (ws, _) = connect_async(request).await.expect("Failed to connect");
    let (mut write, mut read) = ws.split();

    let client = setup_redis().await;
    let mut conn = client.get_async_connection().await.unwrap();
    let channel = format!("session:{}:down", session_id);
    let test_message = r#"{"type":"test","data":"hello"}"#;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let _: () = conn.publish(&channel, test_message).await.unwrap();

    let timeout = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Some(msg) = read.next().await {
            if let Ok(Message::Text(text)) = msg {
                if text.contains("test") {
                    return Some(text);
                }
            }
        }
        None
    }).await;

    assert!(timeout.is_ok());
    let received = timeout.unwrap();
    assert!(received.is_some(), "Should receive downstream message");
    assert!(received.unwrap().contains("hello"));

    write.close().await.ok();
}