//! Scoped input inventory shared by diagnostic campaigns and their verifiers.
use crate::support::sha;
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fs, path::Path};

pub fn snapshot(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "include/weekend_db.h",
        ".cargo/config.toml",
        "docs/decisions/workspaces.json",
        "docs/decisions/current-scope.json",
        "tools/coverage-fixture.rs",
        "tools/admin.rs",
        "tools/benchmark.rs",
        "tools/compatibility-driver.rs",
        "tools/compatibility-snapshot.rs",
        "tools/compatibility-typed-v1.rs",
        "tools/compatibility-typed-v2.rs",
        "tools/snapshot-cli-upgrade-fixture.rs",
        "tools/monthly_workload.rs",
        "tools/study-cli-upgrade-fixture.rs",
    ] {
        let file = root.join(name);
        if file.is_file() {
            out.insert(name.into(), sha(&file)?);
        }
    }
    for name in [
        "src",
        "crates",
        "apps",
        "examples",
        "tests",
        "fuzz",
        "xtask",
        "probe",
        "tools/atomic-model",
        "tools/decision-inventory",
        "tools/replica-progress-fixture",
        "tools/serialization-scale",
        "tools/resource-envelope",
        "tools/sqlite-baseline",
        "tools/soak",
        "tools/monthly-workload",
        "tools/incident-recovery",
    ] {
        walk(root, &root.join(name), &mut out, false)?;
    }
    Ok(out)
}
fn walk(
    root: &Path,
    directory: &Path,
    out: &mut BTreeMap<String, String>,
    all: bool,
) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let file = entry?.path();
        if file == root.join("probe/build") {
            continue;
        }
        if matches!(
            file.file_name().and_then(|s| s.to_str()),
            Some("target" | "corpus" | "artifacts" | ".git")
        ) {
            continue;
        }
        let kind = fs::symlink_metadata(&file)?.file_type();
        ensure!(!kind.is_symlink(), "source symlink: {}", file.display());
        if kind.is_dir() {
            walk(root, &file, out, all)?;
        } else if kind.is_file()
            && (all
                || file.starts_with(root.join("xtask/tests"))
                || matches!(
                    file.extension().and_then(|s| s.to_str()),
                    Some("rs" | "c" | "h" | "json" | "odin")
                )
                || matches!(
                    file.file_name().and_then(|s| s.to_str()),
                    Some("Cargo.toml" | "Cargo.lock" | "legacy-controls.json")
                ))
        {
            out.insert(
                file.strip_prefix(root)?
                    .to_str()
                    .context("UTF-8 source path")?
                    .replace('\\', "/"),
                sha(&file)?,
            );
        }
    }
    Ok(())
}
pub fn seeds(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    walk(root, &root.join("fuzz/seeds"), &mut out, true)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_catalog_and_workspace_sources_are_campaign_inputs() {
        let root = crate::support::Scratch::new(&std::env::temp_dir(), "catalog-inputs").unwrap();
        for file in [
            "docs/decisions/workspaces.json",
            "docs/decisions/current-scope.json",
            "tools/decision-inventory/Cargo.toml",
            "tools/decision-inventory/Cargo.lock",
            "tools/decision-inventory/src/discovery.rs",
            "tools/decision-inventory/tests/catalog.rs",
            "tools/replica-progress-fixture/Cargo.toml",
            "tools/replica-progress-fixture/src/main.rs",
        ] {
            let path = root.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "original").unwrap();
            let before = snapshot(root.path()).unwrap();
            assert_eq!(
                before.get(file),
                Some(&sha(&path).unwrap()),
                "missing {file}"
            );
            fs::write(&path, "changed").unwrap();
            assert_ne!(
                before,
                snapshot(root.path()).unwrap(),
                "ignored change: {file}"
            );
        }
    }

    #[test]
    fn admin_source_changes_are_part_of_campaign_input_identity() {
        let root = crate::support::Scratch::new(&std::env::temp_dir(), "source-inventory").unwrap();
        fs::create_dir(root.path().join("tools")).unwrap();
        let admin = root.path().join("tools/admin.rs");
        fs::write(&admin, "first").unwrap();
        let before = snapshot(root.path()).unwrap();
        assert_eq!(before["tools/admin.rs"], sha(&admin).unwrap());
        fs::write(&admin, "second").unwrap();
        let after = snapshot(root.path()).unwrap();
        assert_ne!(before, after);
        assert_eq!(after["tools/admin.rs"], sha(&admin).unwrap());
    }
}
