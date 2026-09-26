/// Network resource-pricing configuration snapshot management.
pub mod diff;
pub mod history;
pub mod model;
pub mod store;

use std::fs;
use std::path::Path;

pub fn write_snapshot_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
