//! RPC-related CLI value parsers and shared RPC option definitions.

use clap::Args;
use clap::builder::{PossibleValue, TypedValueParser};

/// Value parser for `--network` that advertises the supported network names to
/// shell completion generators and `--help` output while still accepting any
/// value at runtime, preserving the existing `AppError`-based handling of
/// unknown networks (issue #270).
#[derive(Clone, Debug, Default)]
pub struct NetworkValueParser;

impl TypedValueParser for NetworkValueParser {
    type Value = String;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        value.to_str().map(str::to_owned).ok_or_else(|| {
            clap::Error::raw(
                clap::error::ErrorKind::InvalidUtf8,
                "invalid UTF-8: network value must be valid UTF-8",
            )
        })
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(
            ["testnet", "mainnet", "futurenet", "local"]
                .iter()
                .map(|&name| PossibleValue::new(name)),
        ))
    }
}

/// Reusable RPC connection flags shared across network-bound CLI subcommands.
#[derive(Args, Clone, Debug)]
pub struct RpcOptions {
    /// Network to connect to (testnet, mainnet, futurenet, local).
    #[arg(long, default_value = "testnet", value_parser = NetworkValueParser)]
    pub network: String,

    /// Explicit RPC URL (overrides network-based resolution).
    #[arg(long)]
    pub rpc_url: Option<String>,
}
