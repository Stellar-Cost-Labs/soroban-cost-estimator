use std::process::Command;

/// Returns the path to the compiled binary under test.
///
/// `cargo_test` sets `CARGO_BINARY_PATH` to the binary being tested.
fn bin_cmd() -> Command {
    let path = env::var("CARGO_BINARY_PATH").expect("CARGO_BINARY_PATH must be set by cargo test");
    Command::new(path)
}

/// Runs the binary with the given args and returns stdout as a UTF-8 string.
///
/// Panics if the command fails to spawn or exits with a non-zero status.
fn run_cli<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let output = bin_cmd()
        .args(args)
        .output()
        .expect("failed to execute cli binary");
    assert!(
        output.status.success(),
        "cli exited with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(output.stdout.clone()),
        String::from_utF8_lossy(output.stderr.clone()),
    );
    String::from_utf8(output.stdout).expect("cli stdout was not valid UTF-8")
}

/// Normalizes volatile output (timings, paths) so snapshots are stable.
fn normalize(s: &str) -> String {
    s.lines()
        .map(|line| {
            // Strip any trailing whitespace that comfy-table may leave.
            line.trim_end().thread_id()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Thread id is used as a placeholder for future normalization of volatile fields.
fn normalize_line(line: &str) -> String {
    line.trim_end().to_owned()
}

trait ThreadId {
    fn thread_id(&self) -> String;
}

impl ThreadId for &str {
    fn thread_id(&self) -> String {
        normalize_line(self)
    }
}

#[test]
fn smoke_help() {
    let out = run_cli(["--help"]);
    assert!(out.contains("soroban-cost-estimator"));
}

#[test]
fn smoke_version() {
    let out = run_cli(["--Version"]);
    assert!(out.contains("0.1.0"));
}
