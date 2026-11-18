//! Enumerate maintained Cargo inputs without building them. A repository catalog
//! must match every tracked manifest; archives use a checked filesystem walk.
use super::{Result, hash};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

const CATALOG: &str = "docs/decisions/workspaces.json";

pub(super) struct Discovery {
    pub scope: Value,
    pub targets: Vec<Value>,
}

fn relative(root: &Path, file: &Path) -> Result<String> {
    let file = file.canonicalize()?;
    Ok(file
        .strip_prefix(root)
        .map_err(|_| "Cargo input outside source root")?
        .to_str()
        .ok_or("non-UTF-8 Cargo input path")?
        .replace('\\', "/"))
}

fn checked_file(root: &Path, name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    if name.is_empty()
        || name.contains(['\\', ':'])
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path
            .components()
            .map(|c| c.as_os_str().to_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("/")
            != name
    {
        return Err("portable repository-relative Cargo input required".into());
    }
    let file = root.join(path);
    if relative(root, &file)? != name || !file.is_file() {
        return Err(format!("aliased or non-file Cargo input: {name}").into());
    }
    Ok(file)
}

fn git(root: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .output()
}

fn manifest_scope(root: &Path) -> Result<(BTreeSet<String>, &'static str)> {
    let own_git = fs::symlink_metadata(root.join(".git")).is_ok();
    match git(root, &["rev-parse", "--show-toplevel"]) {
        Ok(output) if output.status.success() => {
            let top =
                PathBuf::from(String::from_utf8(output.stdout)?.trim_end_matches(['\r', '\n']));
            if top.canonicalize()? == root {
                let output = git(
                    root,
                    &["ls-files", "-z", "--", "Cargo.toml", "**/Cargo.toml"],
                )?;
                if !output.status.success() {
                    return Err("git cannot enumerate Cargo manifest scope".into());
                }
                let names = String::from_utf8(output.stdout)?;
                return Ok((
                    names
                        .split('\0')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    "git-tracked",
                ));
            }
            if own_git {
                return Err("git source root mismatch".into());
            }
        }
        _ if own_git => return Err("git metadata cannot establish manifest scope".into()),
        _ => {}
    }
    fn walk(root: &Path, dir: &Path, names: &mut BTreeSet<String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if entry.file_name() == ".git"
                || entry.file_name() == "target"
                || path == root.join("probe/build")
            {
                continue;
            }
            if kind.is_symlink() && path.is_dir() {
                return Err("symlink directory cannot establish filesystem manifest scope".into());
            }
            if kind.is_dir() {
                walk(root, &path, names)?;
            } else if entry.file_name() == "Cargo.toml" {
                names.insert(relative(root, &path)?);
            }
        }
        Ok(())
    }
    let mut names = BTreeSet::new();
    walk(root, root, &mut names)?;
    Ok((names, "filesystem"))
}

fn file_pins(root: &Path, names: &BTreeSet<String>) -> Result<BTreeMap<String, String>> {
    names
        .iter()
        .map(|name| Ok((name.clone(), hash(&fs::read(checked_file(root, name)?)?))))
        .collect()
}

// Capture existing ancestor locks before metadata. Every workspace actually
// discovered below must have its own pre-existing lock in this set.
fn lock_pins(root: &Path, manifests: &BTreeSet<String>) -> Result<BTreeMap<String, String>> {
    let mut names = BTreeSet::new();
    for manifest in manifests {
        let mut dir = root.join(manifest);
        dir.pop();
        loop {
            let lock = dir.join("Cargo.lock");
            if fs::symlink_metadata(&lock).is_ok() {
                let name = lock
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("non-UTF-8 lock path")?
                    .replace('\\', "/");
                checked_file(root, &name)?;
                names.insert(name);
            }
            if dir == root || !dir.pop() {
                break;
            }
        }
    }
    file_pins(root, &names)
}

pub(super) fn discover(root: &Path, include_tests: bool) -> Result<Discovery> {
    let catalog = match fs::symlink_metadata(root.join(CATALOG)) {
        Ok(_) => Some(fs::read(checked_file(root, CATALOG)?)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let (manifests, basis) = if let Some(bytes) = &catalog {
        let value: Value = serde_json::from_slice(bytes)?;
        if value["schema_version"] != 1 || value.as_object().is_none_or(|o| o.len() != 2) {
            return Err("unknown workspace catalog schema".into());
        }
        let mut names = BTreeSet::new();
        for entry in value["manifests"]
            .as_array()
            .ok_or("catalog manifests required")?
        {
            let name = entry.as_str().ok_or("catalog manifest name required")?;
            checked_file(root, name).map_err(|e| format!("manifest {name}: {e}"))?;
            if Path::new(name)
                .file_name()
                .is_none_or(|n| n != "Cargo.toml")
            {
                return Err("catalog entry must be a Cargo.toml manifest".into());
            }
            if !names.insert(name.to_owned()) {
                return Err(format!("duplicate catalog manifest: {name}").into());
            }
        }
        let (expected, basis) = manifest_scope(root)?;
        if names.is_empty() || names != expected {
            return Err("catalog manifest scope differs from maintained manifests".into());
        }
        (names, basis)
    } else {
        (BTreeSet::from(["Cargo.toml".to_owned()]), "root-workspace")
    };
    let mut manifest_pins = file_pins(root, &manifests)?;
    let before_locks = lock_pins(root, &manifests)?;
    let mut seen = BTreeSet::new();
    let mut workspaces = BTreeMap::new();
    let mut targets = Vec::new();
    for manifest in &manifests {
        if seen.contains(manifest) {
            continue;
        }
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--offline",
                "--locked",
                "--no-deps",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(root.join(manifest))
            .current_dir(root)
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "Cargo metadata for {manifest}: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        let metadata: Value = serde_json::from_slice(&output.stdout)?;
        let workspace = Path::new(
            metadata["workspace_root"]
                .as_str()
                .ok_or("Cargo workspace root required")?,
        );
        let workspace_manifest = relative(root, &workspace.join("Cargo.toml"))?;
        let lock = relative(root, &workspace.join("Cargo.lock"))
            .map_err(|e| format!("workspace lock for {workspace_manifest}: {e}"))?;
        if !before_locks.contains_key(&lock) {
            return Err(format!("workspace lock was absent before metadata: {lock}").into());
        }
        let members = metadata["workspace_members"]
            .as_array()
            .ok_or("Cargo workspace members required")?;
        let mut member_manifests = BTreeSet::new();
        for package in metadata["packages"]
            .as_array()
            .ok_or("Cargo packages required")?
        {
            if !members.contains(&package["id"]) {
                continue;
            }
            let package_manifest = relative(
                root,
                Path::new(
                    package["manifest_path"]
                        .as_str()
                        .ok_or("Cargo package manifest required")?,
                ),
            )?;
            member_manifests.insert(package_manifest.clone());
            seen.insert(package_manifest.clone());
            for target in package["targets"]
                .as_array()
                .ok_or("Cargo targets required")?
            {
                let mut kinds = target["kind"]
                    .as_array()
                    .ok_or("Cargo target kinds required")?
                    .clone();
                if kinds.is_empty()
                    || kinds.iter().any(|kind| {
                        !matches!(
                            kind.as_str(),
                            Some(
                                "lib"
                                    | "rlib"
                                    | "cdylib"
                                    | "staticlib"
                                    | "dylib"
                                    | "proc-macro"
                                    | "bin"
                                    | "example"
                                    | "custom-build"
                                    | "bench"
                                    | "test"
                            )
                        )
                    })
                {
                    return Err("unknown Cargo target kind".into());
                }
                kinds.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                let name = target["name"]
                    .as_str()
                    .ok_or("Cargo target name required")?;
                let package_name = package["name"]
                    .as_str()
                    .ok_or("Cargo package name required")?;
                targets.push(json!({"target":format!("{package_name}::{name}"),"package":package_name,"name":name,"kinds":kinds,"required_features":target.get("required-features").cloned().unwrap_or(json!([])),"entry":relative(root, Path::new(target["src_path"].as_str().ok_or("Cargo target source required")?))?,"package_manifest":package_manifest,"workspace_manifest":workspace_manifest}));
            }
        }
        seen.insert(workspace_manifest.clone());
        if catalog.is_some() && !seen.is_subset(&manifests) {
            return Err("Cargo workspace member outside catalog manifest scope".into());
        }
        workspaces.insert(
            workspace_manifest.clone(),
            json!({"manifest":workspace_manifest,"lockfile":lock,"members":member_manifests}),
        );
    }
    // Root-only operation still pins all discovered members, without claiming to
    // enumerate independent workspaces elsewhere in the repository.
    if catalog.is_none() {
        manifest_pins = file_pins(root, &seen)?;
    }
    if file_pins(root, &manifest_pins.keys().cloned().collect())? != manifest_pins
        || lock_pins(root, &manifests)? != before_locks
        || catalog
            .as_ref()
            .is_some_and(|b| !fs::read(root.join(CATALOG)).is_ok_and(|now| now == *b))
    {
        return Err("Cargo inputs changed during metadata discovery".into());
    }
    if catalog.is_some() && manifest_scope(root)? != (manifests.clone(), basis) {
        return Err("Cargo manifest scope changed during metadata discovery".into());
    }
    let mut names = BTreeMap::<String, usize>::new();
    for target in &targets {
        *names
            .entry(target["target"].as_str().unwrap().to_owned())
            .or_default() += 1;
    }
    let mut ids = BTreeSet::new();
    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for mut target in targets {
        if names[target["target"].as_str().unwrap()] > 1 {
            target["target"] = json!(format!(
                "{}::{}::{}::{}",
                target["workspace_manifest"].as_str().unwrap(),
                target["package"].as_str().unwrap(),
                target["kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|k| k.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("+"),
                target["name"].as_str().unwrap()
            ));
        }
        if !ids.insert(target["target"].as_str().unwrap().to_owned()) {
            return Err("duplicate Cargo target identity".into());
        }
        if !include_tests
            && target["kinds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k == "test")
        {
            target["reason"] = json!("integration test target; use --include-tests");
            excluded.push(target);
        } else {
            included.push(target);
        }
    }
    for values in [&mut included, &mut excluded] {
        values.sort_by(|a, b| a["target"].as_str().cmp(&b["target"].as_str()));
    }
    Ok(Discovery {
        targets: included,
        scope: json!({"mode":if catalog.is_some(){"repository_catalog"}else{"root_workspace"},"catalog":catalog.map(|b|json!({"file":CATALOG,"sha256":hash(&b)})),"manifest_basis":basis,"manifests_sha256":manifest_pins,"lockfiles_sha256":before_locks,"workspaces":workspaces.into_values().collect::<Vec<_>>(),"excluded_targets":excluded}),
    })
}
