use std::process::Command;

/// Formats a whole-second Unix timestamp as an ISO-8601 UTC string, e.g.
/// `2026-10-01T12:34:56Z`.
///
/// Uses Howard Hinnant's `civil_from_days` algorithm so the build date is
/// available on every platform without shelling out to `date`, which does not
/// exist on Windows.
fn format_utc(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let seconds_of_day = timestamp.rem_euclid(86_400);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    let shifted_days = days + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Runs `command` and returns its trimmed stdout, or `None` when the command is
/// missing, exits non-zero, or emits non-UTF-8 or empty output.
fn command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn main() {
    // Capture git commit hash; fall back to `clean` when the build happens
    // outside a git checkout (e.g. from a crates.io tarball).
    let git_hash = command_stdout("git", &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|| "clean".to_string());

    // Capture build date (UTC).
    let build_date = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |_| "unknown".to_string(),
            |elapsed| format_utc(i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)),
        );

    // Capture the rustc version (just the number, e.g. `1.85.0`).
    let rustc_version = command_stdout("rustc", &["--version"])
        .and_then(|line| line.split_whitespace().nth(1).map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());

    // Capture the target triple Cargo is building for. `TARGET` is set by Cargo
    // for build scripts; `rustc -vV` is the fallback when the script runs
    // standalone.
    let target = std::env::var("TARGET")
        .ok()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            command_stdout("rustc", &["-vV"]).and_then(|output| {
                output.lines().find_map(|line| {
                    line.strip_prefix("host:")
                        .map(str::trim)
                        .map(str::to_string)
                })
            })
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={git_hash}");
    println!("cargo:rustc-env=BUILD_DATE={build_date}");
    println!("cargo:rustc-env=RUSTC_VERSION={rustc_version}");
    println!("cargo:rustc-env=BUILD_TARGET={target}");
}
