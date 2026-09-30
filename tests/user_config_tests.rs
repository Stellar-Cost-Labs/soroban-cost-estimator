//! Integration tests for `config.toml` support (issue #274).
//!
//! Each test drives the real binary with `HOME` and `XDG_CONFIG_HOME` pointed
//! into a per-test temporary directory, so the developer's own config never
//! leaks in. RPC behaviour is observed with a local TCP listener that counts
//! connections; no test contacts a real network.

use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Result of one CLI run.
struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

/// A per-test sandbox: a fake home and a fake XDG config dir.
struct Sandbox {
    home: PathBuf,
    xdg: PathBuf,
}

impl Sandbox {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("sce-ucfg-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let xdg = root.join("xdg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&xdg).unwrap();
        Self { home, xdg }
    }

    /// Writes `$XDG_CONFIG_HOME/soroban-cost-estimator/config.toml`.
    fn write_xdg_config(&self, body: &str) -> PathBuf {
        write(&self.xdg.join("soroban-cost-estimator/config.toml"), body)
    }

    /// Writes `~/.soroban-cost-estimator/config.toml`.
    fn write_home_config(&self, body: &str) -> PathBuf {
        write(&self.home.join(".soroban-cost-estimator/config.toml"), body)
    }

    /// Runs the CLI with `XDG_CONFIG_HOME` set to the sandbox XDG dir.
    fn run(&self, args: &[&str]) -> Run {
        self.run_with(args, true)
    }

    /// Runs the CLI with `XDG_CONFIG_HOME` unset, exercising the home-dir
    /// fallback.
    fn run_without_xdg(&self, args: &[&str]) -> Run {
        self.run_with(args, false)
    }

    fn run_with(&self, args: &[&str], with_xdg: bool) -> Run {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"));
        cmd.args(args)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env_remove("SOROBAN_NETWORK")
            .env_remove("SOROBAN_JSON")
            .env_remove("RUST_LOG");
        if with_xdg {
            cmd.env("XDG_CONFIG_HOME", &self.xdg);
        } else {
            cmd.env_remove("XDG_CONFIG_HOME");
        }
        let out = cmd.output().expect("failed to run CLI");
        Run {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        }
    }
}

fn write(path: &Path, body: &str) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    path.to_path_buf()
}

/// Network that `config list` resolved to, read from its "no snapshots"
/// message. `config list` is offline, so it shows the effective network
/// without any RPC traffic.
fn listed_network(run: &Run) -> String {
    assert_eq!(run.code, 0, "config list failed: {}", run.stderr);
    let prefix = "No snapshots found for network '";
    let rest = run
        .stdout
        .split(prefix)
        .nth(1)
        .unwrap_or_else(|| panic!("unexpected output: {}", run.stdout));
    rest.split('\'').next().unwrap().to_string()
}

/// A local endpoint that counts TCP connections.
///
/// In `hold` mode every connection is kept open without a response, so the
/// client waits until its timeout fires. Otherwise connections are closed
/// immediately, which fails the request quickly.
struct Endpoint {
    url: String,
    hits: Arc<AtomicUsize>,
}

impl Endpoint {
    fn start(hold: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                counter.fetch_add(1, Ordering::SeqCst);
                if hold {
                    held.push(stream);
                } else {
                    let mut buf = [0u8; 1024];
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                    let _ = stream.read(&mut buf);
                }
            }
        });
        Self { url, hits }
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

// ── Discovery ────────────────────────────────────────────────────────────

#[test]
fn no_config_keeps_builtin_defaults() {
    let sb = Sandbox::new("none");
    let run = sb.run(&["config", "list"]);
    assert_eq!(listed_network(&run), "testnet");
}

#[test]
fn xdg_config_is_discovered() {
    let sb = Sandbox::new("xdg");
    sb.write_xdg_config("default_network = \"mainnet\"\n");
    assert_eq!(listed_network(&sb.run(&["config", "list"])), "mainnet");
}

#[test]
fn home_config_is_used_when_xdg_unset() {
    let sb = Sandbox::new("home");
    sb.write_home_config("default_network = \"mainnet\"\n");
    assert_eq!(
        listed_network(&sb.run_without_xdg(&["config", "list"])),
        "mainnet"
    );
}

#[test]
fn xdg_config_takes_precedence_over_home_config() {
    let sb = Sandbox::new("xdg-over-home");
    sb.write_xdg_config("default_network = \"futurenet\"\n");
    sb.write_home_config("default_network = \"mainnet\"\n");
    assert_eq!(listed_network(&sb.run(&["config", "list"])), "futurenet");
}

#[test]
fn explicit_config_flag_replaces_discovery() {
    let sb = Sandbox::new("explicit");
    sb.write_xdg_config("default_network = \"mainnet\"\n");
    let explicit = write(
        &sb.home.join("custom.toml"),
        "default_network = \"futurenet\"\n",
    );
    let run = sb.run(&["--config", explicit.to_str().unwrap(), "config", "list"]);
    assert_eq!(listed_network(&run), "futurenet");
}

#[test]
fn explicit_config_flag_after_subcommand_is_honoured() {
    let sb = Sandbox::new("explicit-late");
    let explicit = write(&sb.home.join("c.toml"), "default_network = \"mainnet\"\n");
    let run = sb.run(&["config", "list", "--config", explicit.to_str().unwrap()]);
    assert_eq!(listed_network(&run), "mainnet");
}

#[test]
fn explicit_missing_config_file_errors() {
    let sb = Sandbox::new("explicit-missing");
    let missing = sb.home.join("nope.toml");
    let run = sb.run(&["--config", missing.to_str().unwrap(), "config", "list"]);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr.contains("config file not found") && run.stderr.contains("nope.toml"),
        "{}",
        run.stderr
    );
}

#[test]
fn malformed_config_is_reported_not_ignored() {
    let sb = Sandbox::new("malformed");
    let path = sb.write_xdg_config("default_network = [unterminated\n");
    let run = sb.run(&["config", "list"]);
    assert_eq!(run.code, 1, "stdout: {}", run.stdout);
    assert!(
        run.stderr.contains("failed to process config")
            && run.stderr.contains(path.to_str().unwrap()),
        "{}",
        run.stderr
    );
}

#[test]
fn invalid_format_value_is_reported() {
    let sb = Sandbox::new("bad-format");
    sb.write_xdg_config("format = \"text\"\n");
    let run = sb.run(&["config", "list"]);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr
            .contains("expected one of: table, json, csv, markdown"),
        "{}",
        run.stderr
    );
}

#[test]
fn help_still_works_with_malformed_config() {
    let sb = Sandbox::new("malformed-help");
    sb.write_xdg_config("not toml at all [[[");
    let run = sb.run(&["--help"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(run.stdout.contains("--config"));
}

// ── Network ──────────────────────────────────────────────────────────────

#[test]
fn cli_network_overrides_config() {
    let sb = Sandbox::new("net-override");
    sb.write_xdg_config("default_network = \"mainnet\"\n");
    let run = sb.run(&["config", "list", "--network", "testnet"]);
    assert_eq!(listed_network(&run), "testnet");
}

#[test]
fn legacy_network_key_now_takes_effect() {
    let sb = Sandbox::new("legacy-net");
    sb.write_xdg_config("network = \"mainnet\"\n");
    assert_eq!(listed_network(&sb.run(&["config", "list"])), "mainnet");
}

// ── Format ───────────────────────────────────────────────────────────────

/// The JSON document `wasm-info` printed, if any. The startup log line also
/// goes to stdout, so parsing starts at the first line that opens an object.
fn wasm_info_json(stdout: &str) -> Option<serde_json::Value> {
    let start = stdout
        .find("\n{")
        .map(|i| i + 1)
        .or_else(|| stdout.starts_with('{').then_some(0))?;
    serde_json::from_str(&stdout[start..]).ok()
}

#[test]
fn config_format_reaches_command_output() {
    let sb = Sandbox::new("fmt");
    sb.write_xdg_config("format = \"json\"\n");
    let run = sb.run(&["wasm-info", "--wasm", "tests/fixtures/minimal.wasm"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    let doc = wasm_info_json(&run.stdout)
        .unwrap_or_else(|| panic!("expected JSON from config format: {}", run.stdout));
    assert_eq!(doc["path"], "tests/fixtures/minimal.wasm");
}

#[test]
fn cli_format_overrides_config() {
    let sb = Sandbox::new("fmt-override");
    sb.write_xdg_config("format = \"json\"\n");
    let run = sb.run(&[
        "wasm-info",
        "--wasm",
        "tests/fixtures/minimal.wasm",
        "--format",
        "table",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        wasm_info_json(&run.stdout).is_none() && run.stdout.contains("minimal.wasm"),
        "--format table must win over config json: {}",
        run.stdout
    );
}

#[test]
fn no_config_format_keeps_table_output() {
    let sb = Sandbox::new("fmt-none");
    let run = sb.run(&["wasm-info", "--wasm", "tests/fixtures/minimal.wasm"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(wasm_info_json(&run.stdout).is_none(), "{}", run.stdout);
    assert!(run.stdout.contains("minimal.wasm"), "{}", run.stdout);
}

// ── RPC URLs ─────────────────────────────────────────────────────────────

/// `estimate` against `minimal.wasm` reaches the RPC with a fresh home.
fn estimate(sb: &Sandbox, extra: &[&str]) -> Run {
    let mut args = vec![
        "estimate",
        "--wasm",
        "tests/fixtures/minimal.wasm",
        "--max-retries",
        "0",
    ];
    args.extend_from_slice(extra);
    sb.run(&args)
}

#[test]
fn configured_network_uses_configured_rpc() {
    let sb = Sandbox::new("rpc-cfg-cfg");
    let ep = Endpoint::start(false);
    sb.write_xdg_config(&format!(
        "default_network = \"mainnet\"\n[rpc_urls]\nmainnet = \"{}\"\n",
        ep.url
    ));
    let run = estimate(&sb, &[]);
    assert_eq!(run.code, 1);
    assert!(ep.hits() > 0, "configured RPC was not used: {}", run.stderr);
}

#[test]
fn explicit_network_uses_configured_rpc_for_that_network() {
    let sb = Sandbox::new("rpc-cli-cfg");
    let mainnet = Endpoint::start(false);
    let testnet = Endpoint::start(false);
    sb.write_xdg_config(&format!(
        "default_network = \"testnet\"\n[rpc_urls]\ntestnet = \"{}\"\nmainnet = \"{}\"\n",
        testnet.url, mainnet.url
    ));
    estimate(&sb, &["--network", "mainnet"]);
    assert!(mainnet.hits() > 0, "mainnet RPC from config was not used");
    assert_eq!(testnet.hits(), 0, "testnet RPC must not be used");
}

#[test]
fn explicit_rpc_url_overrides_configured_rpc() {
    let sb = Sandbox::new("rpc-cfg-cli");
    let configured = Endpoint::start(false);
    let explicit = Endpoint::start(false);
    sb.write_xdg_config(&format!(
        "default_network = \"mainnet\"\n[rpc_urls]\nmainnet = \"{}\"\n",
        configured.url
    ));
    estimate(&sb, &["--rpc-url", &explicit.url]);
    assert!(explicit.hits() > 0, "--rpc-url was not used");
    assert_eq!(configured.hits(), 0, "config RPC must not be used");
}

#[test]
fn explicit_network_and_rpc_url_ignore_config() {
    let sb = Sandbox::new("rpc-cli-cli");
    let configured = Endpoint::start(false);
    let explicit = Endpoint::start(false);
    sb.write_xdg_config(&format!(
        "default_network = \"mainnet\"\n[rpc_urls]\ntestnet = \"{0}\"\nmainnet = \"{0}\"\n",
        configured.url
    ));
    estimate(&sb, &["--network", "testnet", "--rpc-url", &explicit.url]);
    assert!(explicit.hits() > 0);
    assert_eq!(configured.hits(), 0);
}

#[test]
fn configured_rpc_applies_to_commands_without_rpc_url_flag() {
    let sb = Sandbox::new("rpc-snapshot");
    let ep = Endpoint::start(false);
    sb.write_xdg_config(&format!("[rpc_urls]\ntestnet = \"{}\"\n", ep.url));
    let run = sb.run(&["config", "snapshot", "--max-retries", "0"]);
    assert_eq!(run.code, 1);
    assert!(
        ep.hits() > 0,
        "config snapshot ignored [rpc_urls]: {}",
        run.stderr
    );
}

// ── Timeout ──────────────────────────────────────────────────────────────

/// Runs `config snapshot` against an endpoint that never answers and
/// returns how long the CLI took to give up.
fn time_hung_snapshot(sb: &Sandbox, extra: &[&str]) -> Duration {
    let ep = Endpoint::start(true);
    let mut body = format!("[rpc_urls]\ntestnet = \"{}\"\n", ep.url);
    if let Some(cfg_timeout) = extra.first().filter(|s| s.starts_with("cfg=")) {
        body = format!("timeout_secs = {}\n{body}", &cfg_timeout[4..]);
    }
    sb.write_xdg_config(&body);
    let mut args = vec!["config", "snapshot", "--max-retries", "0"];
    args.extend(extra.iter().filter(|s| !s.starts_with("cfg=")));
    let start = Instant::now();
    let run = sb.run(&args);
    let elapsed = start.elapsed();
    assert_eq!(run.code, 1, "hung endpoint should fail: {}", run.stderr);
    assert!(ep.hits() > 0);
    elapsed
}

#[test]
fn config_timeout_reaches_rpc_client() {
    // The built-in timeout is 30s; finishing well inside it proves the
    // config value was used.
    let sb = Sandbox::new("timeout-cfg");
    let elapsed = time_hung_snapshot(&sb, &["cfg=1"]);
    assert!(elapsed < Duration::from_secs(15), "took {elapsed:?}");
}

#[test]
fn cli_timeout_overrides_config() {
    let sb = Sandbox::new("timeout-cli");
    let elapsed = time_hung_snapshot(&sb, &["cfg=120", "--timeout", "1"]);
    assert!(elapsed < Duration::from_secs(15), "took {elapsed:?}");
}
