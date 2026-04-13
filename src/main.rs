mod board;
mod engine;
mod eval;
mod movegen;
mod telemetry;
mod uci;

#[tokio::main]
async fn main() {
    // Load .env file (silently ignore if missing)
    let _ = dotenvy::dotenv();

    // Initialize OpenTelemetry tracing. The guard flushes spans when dropped.
    let _telemetry_guard = telemetry::init();

    // Run the UCI loop on stdin/stdout
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout();
    uci::run_uci_loop(stdin, &mut stdout);

    // _telemetry_guard drops here, flushing all remaining spans
}
