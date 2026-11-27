import re

with open("src/report/fee_calc.rs", "r") as f:
    content = f.read()

content = content.replace(
    "    pub bandwidth_fee_stroops: i64,\n    /// Total resource fee.\n    pub total_stroops: i64,",
    "    pub bandwidth_fee_stroops: i64,\n    /// Base inclusion fee.\n    pub base_fee_stroops: i64,\n    /// Total resource fee.\n    pub total_stroops: i64,"
)

content = content.replace(
    """    let refundable = total_resource_fee.saturating_sub(non_refundable).max(0);

    let total_xlm = stroops_to_xlm(total_resource_fee, precision);

    // Combined storage I/O fee for the report breakdown.
    let storage_fee = read_entry_fee
        .saturating_add(write_entry_fee)
        .saturating_add(read_bytes_fee);

    FeeBreakdown {
        non_refundable_stroops: non_refundable,
        refundable_stroops: refundable,
        cpu_fee_stroops: cpu_fee,
        storage_fee_stroops: storage_fee,
        bandwidth_fee_stroops: bandwidth_fee,
        total_stroops: total_resource_fee,
        total_xlm,
    }""",
    """    let refundable = total_resource_fee.saturating_sub(non_refundable).max(0);

    let base_fee_stroops = if total_resource_fee > 0 { 100 } else { 0 };
    let total_stroops = total_resource_fee.saturating_add(base_fee_stroops);
    let total_xlm = stroops_to_xlm(total_stroops, precision);

    // Combined storage I/O fee for the report breakdown.
    let storage_fee = read_entry_fee
        .saturating_add(write_entry_fee)
        .saturating_add(read_bytes_fee);

    FeeBreakdown {
        non_refundable_stroops: non_refundable,
        refundable_stroops: refundable,
        cpu_fee_stroops: cpu_fee,
        storage_fee_stroops: storage_fee,
        bandwidth_fee_stroops: bandwidth_fee,
        base_fee_stroops,
        total_stroops,
        total_xlm,
    }"""
)

# Update tests
content = content.replace(
    "assert_eq!(breakdown.total_stroops, 5_000);",
    "assert_eq!(breakdown.total_stroops, 5_100);"
)

content = content.replace(
    """        assert_eq!(breakdown.total_stroops, 1_000_000);
        assert_eq!(breakdown.total_xlm, "0.1000000");""",
    """        assert_eq!(breakdown.total_stroops, 1_000_100);
        assert_eq!(breakdown.total_xlm, "0.1000100");"""
)

content = content.replace(
    "assert_eq!(breakdown.total_stroops, 15_427);",
    "assert_eq!(breakdown.total_stroops, 15_527);"
)

with open("src/report/fee_calc.rs", "w") as f:
    f.write(content)
