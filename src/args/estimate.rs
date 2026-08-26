use clap::Args;

/// Simulate a single contract invocation and print the cost report.
#[derive(Args, Debug)]
pub struct EstimateArgs {
    /// Path to the compiled Soroban contract `.wasm` file.
    #[arg(long, short)]
    pub wasm: String,

    /// Network to simulate against.
    #[arg(long, default_value = "testnet")]
    pub network: String,

    /// Explicit RPC URL (overrides network-based resolution).
    #[arg(long)]
    pub rpc_url: Option<String>,

    /// Contract function name to invoke.
    #[arg(long)]
    pub r#fn: Option<String>,

    /// Deployed contract ID (64 hex chars) to invoke. Required when --fn is used.
    #[arg(long)]
    pub id: Option<String>,

    /// Function arguments as key=value pairs (value is type-inferred).
    #[arg(long = "arg", value_name = "KEY=VAL")]
    pub args: Vec<String>,

    /// Output as JSON instead of a human-readable table.
    #[arg(long)]
    pub json: bool,
}

/// Enumerate all public contract functions and estimate each one.
#[derive(Args, Debug)]
pub struct EstimateAllArgs {
    /// Path to the compiled Soroban contract `.wasm` file.
    #[arg(long, short)]
    pub wasm: String,

    /// Network to simulate against.
    #[arg(long, default_value = "testnet")]
    pub network: String,

    /// Deployed contract ID (64 hex chars) to invoke each function against.
    #[arg(long)]
    pub id: Option<String>,

    /// Output as JSON instead of a human-readable list.
    #[arg(long)]
    pub json: bool,
}
