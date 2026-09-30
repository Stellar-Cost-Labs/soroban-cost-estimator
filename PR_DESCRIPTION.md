# Pull Request: Add Fee Breakdown Percentage Columns

## Issue
Fixes #319

## Summary
This PR introduces percentage breakdown columns for cost reports to help contract developers immediately identify the highest-ROI area for cost optimization. It computes the relative percentage contribution of each fee component to the total transaction fee and updates the formatting outputs accordingly.

## Changes
- **Fee Breakdowns:** Updated `FeeBreakdown` in `src/report/fee_calc.rs` to compute exact percentages for the 5 components: CPU instructions, Storage read/write, Transaction size, Base fee, and Rent.
- **Graceful Rounding:** Ensures that the formatted percentages gracefully round to exactly `100.0%` by distributing the fractional remainder to the largest contributing component.
- **Table Formatting:** Updated `TableFormatter` (in `src/report/formatter.rs`) and `format_report_table` (in `src/report/cost_report.rs`) to present the fee breakdown using a structured table layout displaying the raw stroops and their respective percentage, e.g. `1,250 stroops (75.4%)`.
- **Markdown Formatting:** Updated `MarkdownFormatter` to display the formatted percentages smoothly in the Markdown output.
- **JSON Output:** Added the `fee_percentages` map to the serialization of `CostReport` so that the data is cleanly presented in the machine-readable `--json` output.

## Tests
- Updated formatting snapshot expectations for table, json, and markdown formats.
- Verified test cases validate deterministic parsing and serialization with exactly summing percentages.
