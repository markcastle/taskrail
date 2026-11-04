#[tokio::main]
async fn main() {
    if let Err(e) = taskrail_connector_mcp::fake_server::serve_stdio().await {
        eprintln!("fake tracker: {e}");
        std::process::exit(1);
    }
}
