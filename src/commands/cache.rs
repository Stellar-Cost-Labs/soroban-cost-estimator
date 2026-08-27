use crate::args::cache::CacheAction;
use crate::cache;
use crate::error;

/// `cache verify` command: check every cache entry parses as valid JSON.
///
/// Prints a summary line per corrupted entry and exits with code 1 when any
/// entry fails verification, so scripts can treat a corrupt cache as an
/// error. A healthy (or empty) cache exits 0.
///
/// # Network calls
/// None — pure file I/O.
pub fn cmd_cache(action: &CacheAction) -> error::AppResult<()> {
    match action {
        CacheAction::Verify => cmd_cache_verify(),
    }
}

fn cmd_cache_verify() -> error::AppResult<()> {
    use tracing::debug;

    let statuses = cache::verify_cache()?;
    debug!(total = statuses.len(), "verified cache entries");

    if statuses.is_empty() {
        println!("Cache is empty — nothing to verify.");
        return Ok(());
    }

    let total = statuses.len();
    let corrupt: Vec<&cache::CacheEntryStatus> = statuses.iter().filter(|s| !s.valid).collect();

    println!("Checked {total} cache entries.");

    if corrupt.is_empty() {
        println!("All cache entries are valid.");
    } else {
        println!(
            "{} of {total} cache entries failed verification:",
            corrupt.len()
        );
        for status in &corrupt {
            println!("  - {}", status.filename);
        }
        std::process::exit(1);
    }

    Ok(())
}
