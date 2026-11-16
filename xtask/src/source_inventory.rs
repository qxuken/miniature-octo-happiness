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
        "tools/coverage-fixture.rs",
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
        "tools/serialization-scale",
        "tools/resource-envelope",
        "tools/sqlite-baseline",
        "tools/soak",
        "tools/monthly-workload",
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
#[cfg(unix)]
pub fn seeds(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    walk(root, &root.join("fuzz/seeds"), &mut out, true)?;
    Ok(out)
}
