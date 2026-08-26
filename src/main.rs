use soroban_cost_estimator::cli;

#[tokio::main]
async fn main() {
    if let Err(err) = cli::run().await {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
