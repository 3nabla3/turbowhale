mod board;
mod engine;
mod eval;
mod movegen;
mod perft;
mod telemetry;
mod tt;
mod uci;

#[tokio::main]
async fn main() {
    // Load .env file (silently ignore if missing)
    let _ = dotenvy::dotenv();
    let backend_url = std::env::var("OTEL_BACKEND_URL");

    // Initialize OpenTelemetry tracing. The guard flushes spans when dropped.
    let _telemetry_guard = if let Ok(backend_url) = backend_url {
        Some(telemetry::init(&backend_url))
    } else {
        None
    };

    // Run the UCI loop on stdin/stdout
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout();
    uci::run_uci_loop(stdin, &mut stdout);

    // _telemetry_guard drops here, flushing all remaining spans
}
