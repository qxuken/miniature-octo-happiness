//! Keep removed Python controls visible until the complete conversion is done.
#[cfg(test)]
mod tests {
    use serde_json::Value;
    #[test]
    fn retired_suite_controls_map_to_existing_rust_tests() {
        let inventory: Value =
            serde_json::from_str(include_str!("../tests/legacy-controls.json")).unwrap();
        assert_eq!(
            inventory["controls"].as_array().unwrap().len(),
            179,
            "baseline controls must not disappear"
        );
        let mut ids = std::collections::BTreeSet::new();
        for row in inventory["controls"].as_array().unwrap() {
            assert!(
                ids.insert((row["path"].as_str().unwrap(), row["id"].as_str().unwrap())),
                "duplicate legacy ID"
            );
        }
        let listed = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--list", "--format=terse"])
            .output()
            .unwrap();
        assert!(listed.status.success());
        let text = String::from_utf8(listed.stdout).unwrap();
        let names: std::collections::BTreeSet<_> = text
            .lines()
            .filter_map(|s| s.strip_suffix(": test"))
            .collect();
        for row in inventory["controls"].as_array().unwrap() {
            let retired = inventory["retired_suites"]
                .as_array()
                .unwrap()
                .contains(&row["path"]);
            let mapped = row["rust"].as_array().unwrap();
            if retired {
                assert!(
                    !mapped.is_empty(),
                    "unmapped retired control: {}",
                    row["id"]
                );
            }
            for name in mapped {
                if row["unix_only"].as_bool() == Some(true) && !cfg!(unix) {
                    continue;
                }
                if row["linux_only"].as_bool() == Some(true) && !cfg!(target_os = "linux") {
                    continue;
                }
                assert!(
                    names.contains(name.as_str().unwrap()),
                    "missing Rust control: {name}"
                );
            }
        }
    }
}
