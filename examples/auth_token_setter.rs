use redis::AsyncCommands;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: cargo run --example auth_token_setter <session_id> <token>");
        eprintln!(
            "Example: cargo run --example auth_token_setter test-session-123 my-secret-token"
        );
        std::process::exit(1);
    }

    let session_id = &args[1];
    let token = &args[2];

    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    println!("Connecting to Redis at {}", redis_url);

    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection_manager().await?;

    let redis_key = format!("session:{}:auth", session_id);

    let _: () = conn.set(&redis_key, token).await?;

    println!("Auth token set:");
    println!("  Key: {}", redis_key);
    println!("  Token: {}", token);

    Ok(())
}
