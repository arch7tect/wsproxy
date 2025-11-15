use redis::Commands;
use std::thread;
use std::time::Duration;

fn main() {
    let session_id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "test-session-123".to_string());

    let redis_url = "redis://127.0.0.1:6379";
    let channel = format!("session:{}:down", session_id);

    println!("Connecting to Redis at {}", redis_url);
    println!("Publishing to channel: {}", channel);

    let client = redis::Client::open(redis_url).expect("Failed to create Redis client");
    let mut con = client.get_connection().expect("Failed to connect to Redis");

    println!("Publishing messages every 2 seconds...");
    println!("Press Ctrl+C to stop\n");

    let mut counter = 0;

    loop {
        counter += 1;
        let message = format!(
            "{{\"type\": \"data\", \"payload\": \"Message #{}\"}}",
            counter
        );

        match con.publish::<_, _, i32>(&channel, &message) {
            Ok(_) => {
                println!("[{}] Published: {}", counter, message);
            }
            Err(e) => {
                eprintln!("Error publishing message: {}", e);
            }
        }

        thread::sleep(Duration::from_secs(2));
    }
}
