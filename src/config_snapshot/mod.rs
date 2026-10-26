/// Network resource-pricing configuration snapshot management.
///
/// A snapshot captures the ledger's resource-pricing configuration (compute,
/// read, write, and ledger-entry limits along with their fees) at one ledger
/// sequence, so later runs can detect drift and price changes.
pub mod diff;
/// Helpers for reading a network's snapshot history in ledger order.
pub mod history;
/// Snapshot data model: configuration settings, resource limits, and fees.
pub mod model;
/// Persistence for snapshots on disk and in the estimate cache.
pub mod store;
