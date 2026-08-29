use crate::config_snapshot::diff;
use crate::config_snapshot::diff::field_display_name;
use crate::config_snapshot::model::ConfigSnapshot;
use crate::config_snapshot::store;
use crate::error::AppResult;

/// A single field change observed between two consecutive snapshots.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldHistoryEntry {
    pub field_path: String,
    pub timestamp: String,
    pub ledger: u32,
    pub old_value: String,
    pub new_value: String,
    pub is_pricing_change: bool,
}

/// Builds a chronological change log from a set of snapshots.
///
/// Snapshots are sorted by timestamp, then compared pairwise (each snapshot
/// against its immediate predecessor). Each changed field produces one
/// [`FieldHistoryEntry`] stamped with the timestamp/ledger of the *newer*
/// snapshot in the pair — i.e. when the change was observed to have
/// occurred. Fewer than two snapshots yields an empty log, since there is
/// nothing to compare.
pub fn build_change_log_from_snapshots(snapshots: &[ConfigSnapshot]) -> Vec<FieldHistoryEntry> {
    let mut sorted: Vec<&ConfigSnapshot> = snapshots.iter().collect();
    sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let mut log = Vec::new();
    for pair in sorted.windows(2) {
        let (old, new) = (pair[0], pair[1]);
        let field_diff = diff::diff_snapshots(old, new);
        for change in field_diff.changes {
            log.push(FieldHistoryEntry {
                field_path: change.field_path,
                timestamp: new.timestamp.clone(),
                ledger: new.ledger,
                old_value: change.old_value,
                new_value: change.new_value,
                is_pricing_change: change.is_pricing_change,
            });
        }
    }
    log
}

/// Reduces a change log to the single most recent entry per field.
///
/// The input need not be sorted; entries are compared by timestamp.
/// Returned entries are sorted by field path for stable, readable output.
pub fn last_changed_from_log(entries: &[FieldHistoryEntry]) -> Vec<FieldHistoryEntry> {
    use std::collections::HashMap;

    let mut latest: HashMap<&str, &FieldHistoryEntry> = HashMap::new();
    for entry in entries {
        latest
            .entry(entry.field_path.as_str())
            .and_modify(|existing| {
                if entry.timestamp > existing.timestamp {
                    *existing = entry;
                }
            })
            .or_insert(entry);
    }

    let mut result: Vec<FieldHistoryEntry> = latest.into_values().cloned().collect();
    result.sort_by(|a, b| a.field_path.cmp(&b.field_path));
    result
}

/// Loads every stored snapshot for `network` and builds its change log.
///
/// # Network calls
/// None — pure file I/O via [`store::list_snapshots`].
pub fn load_change_log(network: &str) -> AppResult<Vec<FieldHistoryEntry>> {
    let paths = store::list_snapshots(network)?;
    let mut snapshots = Vec::with_capacity(paths.len());
    for path in paths {
        let snapshot = store::load_snapshot_from_path(&path.to_string_lossy())?;
        snapshots.push(snapshot);
    }
    Ok(build_change_log_from_snapshots(&snapshots))
}

/// Formats a change log as a human-readable, chronological listing.
pub fn format_change_log(network: &str, log: &[FieldHistoryEntry]) -> String {
    let mut output = String::new();
    output.push_str(&format!("Config change log: {network}\n\n"));

    if log.is_empty() {
        output.push_str("No changes recorded (need at least two snapshots).\n");
        return output;
    }

    let mut sorted = log.to_vec();
    sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    for entry in &sorted {
        let icon = if entry.is_pricing_change {
            "💰"
        } else {
            "📋"
        };
        let display = field_display_name(&entry.field_path);
        output.push_str(&format!(
            "  {icon} [{}] (ledger {}) {display}\n",
            entry.timestamp, entry.ledger
        ));
        output.push_str(&format!(
            "      {} → {}\n",
            entry.old_value, entry.new_value
        ));
    }

    output
}

/// Formats a "last changed" table: one line per field, most recent first.
pub fn format_last_changed(network: &str, entries: &[FieldHistoryEntry]) -> String {
    let mut output = String::new();
    output.push_str(&format!("Last changed per setting: {network}\n\n"));

    if entries.is_empty() {
        output.push_str("No changes recorded (need at least two snapshots).\n");
        return output;
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    for entry in &sorted {
        let icon = if entry.is_pricing_change {
            "💰"
        } else {
            "📋"
        };
        let display = field_display_name(&entry.field_path);
        output.push_str(&format!(
            "  {icon} {display} — last changed {} (ledger {}): {} → {}\n",
            entry.timestamp, entry.ledger, entry.old_value, entry.new_value
        ));
    }

    output
}
