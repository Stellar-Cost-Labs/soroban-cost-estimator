import os

for file in ["src/report/cost_report.rs", "src/report/formatter.rs", "src/main.rs"]:
    if not os.path.exists(file): continue
    with open(file, "r") as f:
        content = f.read()
    
    content = content.replace("""                bandwidth_fee_stroops: 61,
                total_stroops: 15_427,
                total_xlm: "0.0015427".to_string(),
            },""", """                bandwidth_fee_stroops: 61,
                base_fee_stroops: 100,
                total_stroops: 15_527,
                total_xlm: "0.0015527".to_string(),
                fee_percentages: std::collections::BTreeMap::new(),
            },""")

    content = content.replace("""                bandwidth_fee_stroops: 0,
                total_stroops: 0,
                total_xlm: "0.0000000".to_string(),
            },""", """                bandwidth_fee_stroops: 0,
                base_fee_stroops: 0,
                total_stroops: 0,
                total_xlm: "0.0000000".to_string(),
                fee_percentages: std::collections::BTreeMap::new(),
            },""")

    with open(file, "w") as f:
        f.write(content)
