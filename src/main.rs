#[tokio::main]
async fn main() {
    if let Err(e) = syncraft_core::client::script::run().await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
