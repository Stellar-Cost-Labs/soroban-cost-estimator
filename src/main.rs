use soroban_cost_estimator::cli;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    if let Err(err) = cli::run().await {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
