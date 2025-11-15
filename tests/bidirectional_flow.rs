use futures::{SinkExt, StreamExt};
use http::{header::AUTHORIZATION, HeaderValue};
use redis::AsyncCommands;
use tokio::time::{sleep, timeout, Duration};
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

#[tokio::test]
async fn test_client_to_server_upstream() {
    let session_id = "test-upstream";
    let token = "upstream-token";
    let url = format!("ws://127.0.0.1:4040/test-agent/ws/{}", session_id);

    set_auth_token(session_id, token).await;

    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );

    let (ws, _) = connect_async(request).await.expect("Failed to connect");
    let (mut write, mut _read) = ws.split();

    let client = setup_redis().await;
    let conn = client.get_async_connection().await.unwrap();
    let mut pubsub = conn.into_pubsub();
    let channel = format!("session:{}:up", session_id);
    pubsub.subscribe(&channel).await.unwrap();

    let test_message = r#"{"type":"ping","timestamp":"2024-01-01T00:00:00Z"}"#;
    write.send(Message::Text(test_message.to_string())).await.unwrap();

    let received = timeout(Duration::from_secs(2), async {
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload().unwrap();
            if payload.contains("ping") {
                return Some(payload);
            }
        }
        None
    }).await;

    assert!(received.is_ok(), "Should receive timeout result");
    let message = received.unwrap();
    assert!(message.is_some(), "Should receive upstream message");
    assert!(message.unwrap().contains("ping"));

    write.close().await.ok();
}

#[tokio::test]
async fn test_invalid_json_rejected() {
    let session_id = "test-invalid-json";
    let token = "json-token";
    let url = format!("ws://127.0.0.1:4040/test-agent/ws/{}", session_id);

    set_auth_token(session_id, token).await;

    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );

    let (ws, _) = connect_async(request).await.expect("Failed to connect");
    let (mut write, mut read) = ws.split();

    let invalid_json = "this is not json";
    write.send(Message::Text(invalid_json.to_string())).await.unwrap();

    let response = timeout(Duration::from_secs(2), async {
        while let Some(msg) = read.next().await {
            if let Ok(Message::Text(text)) = msg {
                if text.contains("error") || text.contains("Invalid JSON") {
                    return Some(text);
                }
            }
        }
        None
    }).await;

    assert!(response.is_ok());
    let error_msg = response.unwrap();
    assert!(error_msg.is_some(), "Should receive error message for invalid JSON");

    write.close().await.ok();
}

#[tokio::test]
async fn test_message_size_within_limit() {
    let session_id = "test-normal-msg";
    let token = "normal-token";
    let url = format!("ws://127.0.0.1:4040/test-agent/ws/{}", session_id);

    set_auth_token(session_id, token).await;

    let client = setup_redis().await;
    let sub_conn = client.get_async_connection().await.unwrap();
    let mut pubsub = sub_conn.into_pubsub();
    let up_channel = format!("session:{}:up", session_id);
    pubsub.subscribe(&up_channel).await.unwrap();

    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );

    let (ws, _) = connect_async(request).await.expect("Failed to connect");
    let (mut write, _read) = ws.split();

    sleep(Duration::from_millis(100)).await;

    let normal_message = format!("{{\"data\":\"{}\"}}", "x".repeat(100 * 1024));
    write.send(Message::Text(normal_message.clone())).await.unwrap();

    let received = timeout(Duration::from_secs(2), async {
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload().unwrap();
            return Some(payload);
        }
        None
    }).await;

    assert!(received.is_ok(), "Should not timeout");
    assert!(received.unwrap().is_some(), "Should receive message within size limit");

    write.close().await.ok();
}

#[tokio::test]
async fn test_full_bidirectional_flow() {
    let session_id = "test-bidir-full";
    let token = "bidir-token";
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
    let mut pub_conn = client.get_async_connection().await.unwrap();
    let sub_conn = client.get_async_connection().await.unwrap();
    let mut pubsub = sub_conn.into_pubsub();

    let up_channel = format!("session:{}:up", session_id);
    let down_channel = format!("session:{}:down", session_id);

    pubsub.subscribe(&up_channel).await.unwrap();

    let upstream_msg = r#"{"type":"request","id":1}"#;
    write.send(Message::Text(upstream_msg.to_string())).await.unwrap();

    sleep(Duration::from_millis(100)).await;

    let downstream_msg = r#"{"type":"response","id":1,"data":"result"}"#;
    let _: () = pub_conn.publish(&down_channel, downstream_msg).await.unwrap();

    let (upstream_received, downstream_received) = tokio::join!(
        timeout(Duration::from_secs(2), async {
            let mut stream = pubsub.on_message();
            while let Some(msg) = stream.next().await {
                let payload: String = msg.get_payload().unwrap();
                if payload.contains("request") {
                    return Some(payload);
                }
            }
            None
        }),
        timeout(Duration::from_secs(2), async {
            while let Some(msg) = read.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if text.contains("response") {
                        return Some(text);
                    }
                }
            }
            None
        })
    );

    assert!(upstream_received.is_ok());
    assert!(upstream_received.unwrap().is_some(), "Should receive upstream message in Redis");

    assert!(downstream_received.is_ok());
    assert!(downstream_received.unwrap().is_some(), "Should receive downstream message in WebSocket");

    write.close().await.ok();
}