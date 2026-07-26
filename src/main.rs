mod client;

#[tokio::main]
async fn main() {
    if let Err(e) = client::script::run().await {
        eprintln!("Error: {e}");
    }
}
