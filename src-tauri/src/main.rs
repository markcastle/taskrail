// Keep the console window away on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The demo connector runs this same binary as its server. Nothing about
    // the UI is touched in that mode.
    if std::env::args().any(|a| a == "--fake-mcp-server") {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        if let Err(e) = runtime.block_on(taskrail_connector_mcp::fake_server::serve_stdio()) {
            eprintln!("fake tracker: {e}");
            std::process::exit(1);
        }
        return;
    }
    taskrail_app::run()
}
