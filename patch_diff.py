import re

with open("src/config_snapshot/diff.rs", "r") as f:
    content = f.read()

# Change pricing_change_color signature
content = re.sub(
    r"pub fn pricing_change_color\(old_value: &str, new_value: &str\) -> &'static str \{",
    "pub fn pricing_change_color(old_value: &str, new_value: &str, colorize: bool) -> &'static str {\n    if !colorize { return \"\"; }",
    content
)

# Change format_diff signature
content = re.sub(
    r"pub fn format_diff\(diff: &ConfigDiff\) -> String \{",
    "pub fn format_diff(diff: &ConfigDiff, colorize: bool) -> String {",
    content
)

# Replace pricing_change_color calls in format_diff
content = re.sub(
    r"pricing_change_color\(&change\.old_value, &change\.new_value\)",
    "pricing_change_color(&change.old_value, &change.new_value, colorize)",
    content
)

# Replace ANSI_RESET usage in format_diff
content = content.replace("ANSI_RESET", "reset_code")
content = re.sub(
    r"pub fn format_diff\(diff: &ConfigDiff, colorize: bool\) -> String \{",
    "pub fn format_diff(diff: &ConfigDiff, colorize: bool) -> String {\n    let reset_code = if colorize { ANSI_RESET } else { \"\" };",
    content
)

# Now fix the test calls
content = re.sub(
    r"pricing_change_color\(([^,]+), ([^)]+)\)",
    r"pricing_change_color(\1, \2, true)",
    content
)
# But wait, in the regex above, we already changed the one in format_diff. Let's fix that up
content = content.replace("pricing_change_color(&change.old_value, &change.new_value, true, colorize)", "pricing_change_color(&change.old_value, &change.new_value, colorize)")
content = content.replace("pricing_change_color(&change.old_value, &change.new_value, true)", "pricing_change_color(&change.old_value, &change.new_value, colorize)")

# Fix format_diff calls in tests
content = re.sub(
    r"format_diff\(&diff\)",
    r"format_diff(&diff, true)",
    content
)

# Also fix the ANSI_RESET reference in test_format_diff_colors_pricing_changes
content = content.replace("assert!(\n            output.contains(reset_code),\n            \"color should be reset after each change\"\n        );", "assert!(\n            output.contains(ANSI_RESET),\n            \"color should be reset after each change\"\n        );")

with open("src/config_snapshot/diff.rs", "w") as f:
    f.write(content)
