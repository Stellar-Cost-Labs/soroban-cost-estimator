//! Versioned, offline-validated batch estimate manifests.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, AppResult},
    rpc, wasm, xdr_helper,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    contracts: Vec<toml::Spanned<toml::Value>>,
}

/// One invocation. After loading, paths and endpoints are resolved.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Path to the compiled WASM contract.
    pub wasm: PathBuf,
    /// Name of the contract function to invoke.
    pub function: String,
    /// Ordered list of arguments for the function invocation.
    pub args: Vec<String>,
    /// Optional network name override for this invocation.
    pub network: Option<String>,
    /// Optional RPC endpoint URL override for this invocation.
    pub rpc_url: Option<String>,
    /// Optional deployed contract ID (hex or strkey).
    pub id: Option<String>,
}

fn location(
    path: &Path,
    source: &str,
    offset: usize,
    entry: &str,
    message: impl std::fmt::Display,
) -> AppError {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
    let column = prefix
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count()
        + 1;
    AppError::General(format!(
        "{}:{line}:{column}: {entry}: {message}",
        path.display()
    ))
}

/// Validate the entire suite without contacting any RPC endpoint.
pub fn load(path: &Path, network: &str, rpc_url: Option<&str>) -> AppResult<Vec<Entry>> {
    let source = std::fs::read_to_string(path).map_err(|e| location(path, "", 0, "manifest", e))?;
    let manifest: Manifest = toml::from_str(&source).map_err(|e: toml::de::Error| {
        location(
            path,
            &source,
            e.span().map_or(0, |s| s.start),
            "manifest",
            e.message(),
        )
    })?;
    if manifest.version != 1 {
        return Err(location(
            path,
            &source,
            0,
            "manifest",
            format!(
                "unsupported version {}; supported version is 1",
                manifest.version
            ),
        ));
    }
    if manifest.contracts.is_empty() {
        return Err(location(
            path,
            &source,
            0,
            "manifest",
            "contracts must not be empty",
        ));
    }
    manifest
        .contracts
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let offset = value.span().start;
            let label = format!("contracts[{}]", index + 1);
            let validate = || -> AppResult<Entry> {
                let mut entry: Entry = value
                    .into_inner()
                    .try_into()
                    .map_err(|e| AppError::General(format!("{e}")))?;
                entry.wasm = path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(&entry.wasm);
                let info = wasm::parser::load_wasm(&entry.wasm)?;
                let function = info
                    .functions
                    .iter()
                    .find(|f| f.name == entry.function)
                    .ok_or_else(|| {
                        AppError::General(format!("unknown function '{}'", entry.function))
                    })?;
                if entry.args.len() != function.param_count as usize {
                    return Err(AppError::General(format!(
                        "function '{}' expects {} arguments, got {}",
                        entry.function,
                        function.param_count,
                        entry.args.len()
                    )));
                }
                xdr_helper::validate_args_against_spec(
                    Some(&entry.function),
                    &entry.args,
                    &info.functions,
                )?;
                let vals: Vec<_> = entry
                    .args
                    .iter()
                    .map(|a| xdr_helper::parse_arg_scval(a))
                    .collect();
                xdr_helper::build_simulation_tx_envelope(
                    &info.bytes,
                    entry.id.as_deref(),
                    Some(&entry.function),
                    &vals,
                )?;
                let selected_network = entry.network.as_deref().unwrap_or(network);
                // An entry's network override must not inherit an unrelated default URL.
                let selected_url = entry.rpc_url.as_deref().or(if entry.network.is_none() {
                    rpc_url
                } else {
                    None
                });
                let endpoint = rpc::client::resolve_endpoint(selected_network, selected_url)?;
                let url = reqwest::Url::parse(&endpoint)
                    .map_err(|e| AppError::General(format!("invalid RPC URL: {e}")))?;
                if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                    return Err(AppError::General(
                        "RPC URL must use HTTP or HTTPS and include a host".into(),
                    ));
                }
                entry.network = Some(selected_network.to_owned());
                entry.rpc_url = Some(endpoint);
                Ok(entry)
            };
            validate().map_err(|e| location(path, &source, offset, &label, e))
        })
        .collect()
}
