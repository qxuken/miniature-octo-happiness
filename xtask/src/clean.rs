use anyhow::{Result, ensure};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

fn no_symlinks(root: &Path, target: &Path) -> Result<bool> {
    let mut p = root.to_owned();
    for component in target.strip_prefix(root)?.components() {
        p.push(component);
        match fs::symlink_metadata(&p) {
            Ok(meta) => ensure!(
                !meta.file_type().is_symlink(),
                "refusing symlink in cleanup path: {}",
                p.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
}

fn plan(root: &Path, tracked: &str, proofs: bool, executable: &Path) -> Result<Vec<PathBuf>> {
    let target = root.join("target");
    let mut candidates = BTreeSet::new();
    if no_symlinks(root, &target)? {
        for entry in fs::read_dir(&target)? {
            let entry = entry?;
            let name = entry.file_name();
            if name == "xtask" {
                continue;
            }
            if proofs
                || ["debug", "release", ".rustc_info.json", "CACHEDIR.TAG"]
                    .iter()
                    .any(|n| name == *n)
            {
                candidates.insert(entry.path());
            }
        }
    }
    let files: Vec<PathBuf> = tracked
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(|s| root.join(s))
        .collect();
    for manifest in &files {
        if manifest.file_name() == Some(std::ffi::OsStr::new("Cargo.toml"))
            && manifest.parent() != Some(root)
        {
            let nested = manifest.parent().unwrap().join("target");
            if no_symlinks(root, &nested)? {
                candidates.insert(nested);
            }
        }
    }
    for p in &candidates {
        ensure!(
            p.starts_with(root) && p != root && !executable.starts_with(p),
            "cleanup would remove the running task or repository: {}",
            p.display()
        );
        no_symlinks(root, p)?;
        ensure!(
            !files.iter().any(|f| f.starts_with(p)),
            "tracked source in cleanup path: {}",
            p.display()
        );
    }
    // A nested proof checkout can contain another Cargo target. Remove its
    // outer owned output once, after validating every candidate above.
    Ok(candidates
        .iter()
        .filter(|p| {
            !candidates
                .iter()
                .any(|other| other != *p && p.starts_with(other))
        })
        .cloned()
        .collect())
}

pub fn run(root: &Path, tracked: &str, proofs: bool, dry: bool, executable: &Path) -> Result<()> {
    let paths = plan(root, tracked, proofs, executable)?;
    for p in paths {
        println!(
            "{} {}",
            if dry { "Would remove" } else { "Removing" },
            p.display()
        );
        if !dry {
            if p.is_dir() {
                fs::remove_dir_all(p)?;
            } else {
                fs::remove_file(p)?;
            }
        }
    }
    println!(
        "Kept target/xtask{}; external build directories were not followed.",
        if proofs {
            ""
        } else {
            " and saved root-target qualification outputs"
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    #[test]
    fn cleanup_keeps_proofs_and_runner_until_explicitly_requested() {
        let mut temp = Scratch::new(&std::env::temp_dir(), "clean-test").unwrap();
        let root = temp.path();
        for n in [
            "target/debug",
            "target/release",
            "target/xtask/debug",
            "target/proof",
            "fuzz/target",
        ] {
            fs::create_dir_all(root.join(n)).unwrap();
            fs::write(root.join(n).join("marker"), b"keep or delete").unwrap();
        }
        let files = "Cargo.toml\0fuzz/Cargo.toml\0";
        let exe = root.join("target/xtask/debug/xtask");
        run(root, files, false, true, &exe).unwrap();
        assert!(root.join("target/debug/marker").exists());
        run(root, files, false, false, &exe).unwrap();
        assert!(!root.join("target/debug").exists());
        assert!(!root.join("fuzz/target").exists());
        assert!(root.join("target/proof/marker").exists());
        assert!(root.join("target/xtask/debug/marker").exists());
        run(root, files, true, false, &exe).unwrap();
        assert!(!root.join("target/proof").exists());
        assert!(exe.parent().unwrap().exists());
        temp.close().unwrap();
    }
    #[test]
    fn all_paths_are_validated_before_removing_any() {
        let mut temp = Scratch::new(&std::env::temp_dir(), "clean-test").unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::create_dir_all(root.join("target/proof")).unwrap();
        assert!(
            run(
                root,
                "target/proof/source.rs\0",
                true,
                false,
                Path::new("/outside/task")
            )
            .is_err()
        );
        assert!(root.join("target/debug").exists());
        assert!(run(root, "", false, false, &root.join("target/debug/xtask")).is_err());
        assert!(root.join("target/debug").exists());
        temp.close().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn linked_targets_never_delete_their_destination() {
        let mut temp = Scratch::new(&std::env::temp_dir(), "clean-test").unwrap();
        let mut outside = Scratch::new(&std::env::temp_dir(), "clean-external").unwrap();
        fs::write(outside.path().join("marker"), b"preserve").unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("target")).unwrap();
        assert!(run(temp.path(), "", true, false, Path::new("/outside/task")).is_err());
        assert!(outside.path().join("marker").exists());
        temp.close().unwrap();
        outside.close().unwrap();
    }
}
