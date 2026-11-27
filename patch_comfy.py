import re

def insert_color_logic(filepath):
    with open(filepath, "r") as f:
        content = f.read()
    
    # We want to replace `comfy_table::Table::new()` or `Table::new()`
    # with the same plus the color logic.
    def replacer(match):
        prefix = match.group(0)
        return prefix + "\n    if crate::cli::should_colorize() { table.enforce_styling(); } else { table.force_no_tty(); }"

    # Because `Table::new()` might be `let mut table = Table::new();`
    # Let's match `let mut table = Table::new();` or `let mut table = comfy_table::Table::new();`
    content = re.sub(
        r"(let mut table = (?:comfy_table::)?Table::new\(\);)",
        replacer,
        content
    )

    with open(filepath, "w") as f:
        f.write(content)

insert_color_logic("src/main.rs")
insert_color_logic("src/report/formatter.rs")
insert_color_logic("src/report/cost_report.rs")

