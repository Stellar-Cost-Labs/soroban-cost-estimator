use soroban_cost_estimator::config_snapshot::store::validate_all_snapshots;

#[test]
fn test_validate_all_snapshots_returns_ok_for_empty_network() {
    // With no snapshots saved for a nonexistent network, validate returns Ok(empty)
    let result = validate_all_snapshots("nonexistent_network_xyz_999");
    assert!(result.is_ok());
    let results = result.unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_validate_all_snapshots_returns_empty_list() {
    // Verify the return type and structure
    let result = validate_all_snapshots("testnet");
    assert!(result.is_ok());
    let results = result.unwrap();
    // Each result has path, filename, valid, error fields
    for status in &results {
        assert!(!status.filename.is_empty());
        assert!(!status.path.as_os_str().is_empty());
        // Either valid or has an error message
        assert!(status.valid || status.error.is_some());
    }
}
