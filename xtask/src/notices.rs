//! Attribution collection from actual Cargo compiler-artifact package IDs.
//! Declarations and bounded copied texts are evidence, never legal approval.
use crate::support::{sha, text};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
const MAX_NOTICE: u64 = 8 * 1024 * 1024;
const MAX_TOTAL: u64 = 64 * 1024 * 1024;
pub(crate) fn resolve(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    if path.exists() {
        return Ok(dunce::canonicalize(path)?);
    }
    let mut missing = vec![];
    let mut ancestor = path.as_path();
    while !ancestor.exists() {
        missing.push(
            ancestor
                .file_name()
                .context("notice path has no existing ancestor")?
                .to_owned(),
        );
        ancestor = ancestor.parent().context("notice path parent")?;
    }
    let mut result = dunce::canonicalize(ancestor)?;
    for part in missing.into_iter().rev() {
        if part == ".." {
            result.pop();
        } else if part != "." {
            result.push(part);
        }
    }
    Ok(result)
}
fn relative(path: &Path, base: &Path) -> PathBuf {
    let a: Vec<_> = path.components().collect();
    let b: Vec<_> = base.components().collect();
    let n = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let mut p = PathBuf::new();
    for _ in n..b.len() {
        p.push("..");
    }
    for part in &a[n..] {
        p.push(part.as_os_str());
    }
    p
}
fn notice(name: &str) -> bool {
    static REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    REGEX
        .get_or_init(|| {
            Regex::new(r"(?i)^(licen[cs]es?|copying|copyright|notices?)(?:[._-]|$)").unwrap()
        })
        .is_match(name)
}
type NoticePaths = Vec<(PathBuf, PathBuf)>;
fn candidates(
    package: &Value,
    workspace: &Path,
    destination: &Path,
) -> Result<(NoticePaths, Vec<String>)> {
    let manifest = Path::new(text(&package["manifest_path"])?);
    let root = resolve(manifest.parent().context("manifest parent")?)?;
    let mut found = BTreeMap::new();
    if let Some(file) = package["license_file"].as_str().filter(|s| !s.is_empty()) {
        found.insert(root.join(file), root.clone());
    }
    let mut boundaries = vec![root.clone()];
    if package["source"].is_null() && root.starts_with(workspace) && root != workspace {
        boundaries.push(workspace.to_owned());
    }
    fn walk(
        boundary: &Path,
        current: &Path,
        workspace: &Path,
        destination: &Path,
        found: &mut BTreeMap<PathBuf, PathBuf>,
    ) -> Result<()> {
        let in_notice = current
            .strip_prefix(boundary)?
            .components()
            .any(|p| notice(&p.as_os_str().to_string_lossy()));
        let mut entries: Vec<_> = fs::read_dir(current)?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let p = entry.path();
            let kind = entry.file_type()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_dir() {
                if name == "target" || name == ".git" || resolve(&p)?.starts_with(destination) {
                    continue;
                }
                if boundary == workspace && !notice(&name) {
                    continue;
                }
                walk(boundary, &p, workspace, destination, found)?;
            } else if !(kind.is_symlink() && p.is_dir()) && (notice(&name) || in_notice) {
                found.insert(p, boundary.to_owned());
            }
        }
        Ok(())
    }
    for boundary in &boundaries {
        walk(boundary, boundary, workspace, destination, &mut found)?;
    }
    let mut paths = vec![];
    let mut problems = vec![];
    for (path, boundary) in found {
        let resolved = resolve(&path);
        let label = relative(&path, &boundary).to_string_lossy().into_owned();
        match resolved {
            Ok(p) if !p.starts_with(&boundary) => {
                problems.push(format!("notice path escapes its allowed root: {label}"))
            }
            Ok(p) if p.is_file() => paths.push((path, boundary)),
            _ => problems.push(format!("declared notice is missing or not a file: {label}")),
        }
    }
    Ok((paths, problems))
}
struct Copies<'a> {
    destination: &'a Path,
    total: u64,
}
impl Copies<'_> {
    fn copy(&mut self, source: &Path, relative: &Path) -> Result<Value> {
        let size = fs::metadata(source)?.len();
        ensure!(
            size <= MAX_NOTICE && self.total.checked_add(size).is_some_and(|n| n <= MAX_TOTAL),
            "attribution files exceed bounded copy budget"
        );
        self.total += size;
        let target = self.destination.join(relative);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(source, &target)?;
        Ok(json!({"file":relative,"sha256":sha(&target)?,"bytes":size}))
    }
}
pub fn collect(
    metadata: &Value,
    ids: &BTreeSet<String>,
    lockfile: &Path,
    destination: &Path,
    sysroot: &Path,
    compiler: &Value,
) -> Result<Value> {
    fs::create_dir_all(
        destination
            .parent()
            .context("attribution destination parent")?,
    )?;
    fs::create_dir(destination)?;
    let workspace = resolve(Path::new(text(&metadata["workspace_root"])?))?;
    let destination = resolve(destination)?;
    let packages: BTreeMap<_, _> = metadata["packages"]
        .as_array()
        .context("Cargo packages")?
        .iter()
        .map(|p| Ok((text(&p["id"])?, p)))
        .collect::<Result<_>>()?;
    ensure!(
        !ids.is_empty() && ids.iter().all(|id| packages.contains_key(id.as_str())),
        "built package IDs must all resolve through Cargo metadata"
    );
    let lock: toml::Table = toml::from_str(&fs::read_to_string(lockfile)?)?;
    let locked = lock["package"]
        .as_array()
        .context("Cargo.lock package array")?;
    let mut inventory = json!({"scope":"Actual compiler-artifact packages for the library, including build dependencies and proc macros.","legal_review":"Unreviewed. Collected declarations and notices do not constitute license approval.","packages":[],"review_items":[],"standard_library":{"compiler":compiler,"notice_files":[]}});
    let mut copies = Copies {
        destination: &destination,
        total: 0,
    };
    for id in ids {
        let package = packages[id.as_str()];
        let records: Vec<_> = locked
            .iter()
            .filter(|entry| {
                entry.get("name").and_then(toml::Value::as_str) == package["name"].as_str()
                    && entry.get("version").and_then(toml::Value::as_str)
                        == package["version"].as_str()
                    && entry.get("source").and_then(toml::Value::as_str)
                        == package["source"].as_str()
            })
            .collect();
        ensure!(
            records.len() == 1,
            "package does not map uniquely to Cargo.lock: {id}"
        );
        let checksum = records[0].get("checksum").and_then(toml::Value::as_str);
        let mut item = json!({"checksum":checksum,"manifest_sha256":sha(Path::new(text(&package["manifest_path"])?))?,"notice_files":[]});
        for key in ["id", "name", "version", "license", "license_file", "source"] {
            item[key] = package[key].clone();
        }
        let folder = PathBuf::from("packages").join(format!("{:x}", Sha256::digest(id.as_bytes())));
        let (paths, mut problems) = candidates(package, &workspace, &destination)?;
        let package_root = resolve(
            Path::new(text(&package["manifest_path"])?)
                .parent()
                .unwrap(),
        )?;
        for (index, (path, boundary)) in paths.iter().enumerate() {
            let mut file = copies.copy(
                path,
                &folder
                    .join(index.to_string())
                    .join(path.file_name().context("notice file name")?),
            )?;
            file["source_relative_path"] = json!(path.strip_prefix(boundary)?);
            file["source_scope"] = json!(if *boundary == package_root {
                "package"
            } else {
                "workspace"
            });
            item["notice_files"].as_array_mut().unwrap().push(file);
        }
        if package["license"].as_str().is_none_or(str::is_empty)
            && package["license_file"].as_str().is_none_or(str::is_empty)
        {
            problems.push("no declared license expression or license-file".into());
        }
        if item["notice_files"].as_array().unwrap().is_empty() {
            problems.push("no license or notice text found".into());
        }
        if package["source"]
            .as_str()
            .is_some_and(|s| s.starts_with("registry+"))
            && checksum.is_none_or(str::is_empty)
        {
            problems.push("registry package has no Cargo.lock checksum".into());
        }
        for problem in problems {
            inventory["review_items"]
                .as_array_mut()
                .unwrap()
                .push(json!({"package":id,"reason":problem}));
        }
        inventory["packages"].as_array_mut().unwrap().push(item);
    }
    let standard = sysroot.join("share/doc/rust/COPYRIGHT-library.html");
    if standard.is_file() && resolve(&standard)?.starts_with(resolve(sysroot)?) {
        inventory["standard_library"]["notice_files"]
            .as_array_mut()
            .unwrap()
            .push(copies.copy(&standard, Path::new("rust-stdlib/COPYRIGHT-library.html"))?);
    } else {
        inventory["review_items"].as_array_mut().unwrap().push(json!({"package":"rust-stdlib","reason":"matching toolchain COPYRIGHT-library.html is missing"}));
    }
    inventory["attribution_inputs_complete"] =
        json!(inventory["review_items"].as_array().unwrap().is_empty());
    fs::write(
        destination.join("inventory.json"),
        serde_json::to_string_pretty(&inventory)? + "\n",
    )?;
    Ok(inventory)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    #[test]
    fn actual_packages_not_unused_metadata_and_review_gates() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "notices").unwrap();
        let root = scratch.path();
        let package = root.join("package");
        fs::create_dir(&package).unwrap();
        fs::write(package.join("Cargo.toml"), "[package]\nname=\"sample\"\n").unwrap();
        fs::write(package.join("LICENSE-MIT"), "example attribution text").unwrap();
        let source = "registry+https://example.invalid/index";
        let mut item = json!({"id":"opaque-sample-id","name":"sample","version":"1.0.0","manifest_path":package.join("Cargo.toml"),"source":source,"license":"MIT","license_file":null});
        let mut unused = item.clone();
        unused["id"] = json!("unused-app");
        let mut metadata =
            json!({"workspace_root":root.join("workspace"),"packages":[item,unused]});
        let lock = root.join("Cargo.lock");
        fs::write(&lock,format!("[[package]]\nname=\"sample\"\nversion=\"1.0.0\"\nsource=\"{source}\"\nchecksum=\"abc123\"\n")).unwrap();
        let sysroot = root.join("sysroot");
        let standard = sysroot.join("share/doc/rust/COPYRIGHT-library.html");
        fs::create_dir_all(standard.parent().unwrap()).unwrap();
        fs::write(&standard, "stdlib attribution").unwrap();
        let ids = BTreeSet::from(["opaque-sample-id".into()]);
        let inventory = collect(
            &metadata,
            &ids,
            &lock,
            &root.join("output"),
            &sysroot,
            &json!("test rustc"),
        )
        .unwrap();
        assert_eq!(inventory["attribution_inputs_complete"], true);
        assert_eq!(inventory["packages"].as_array().unwrap().len(), 1);
        assert_eq!(inventory["packages"][0]["checksum"], "abc123");
        let record = &inventory["packages"][0]["notice_files"][0];
        let copied = root.join("output").join(record["file"].as_str().unwrap());
        assert_eq!(
            fs::read_to_string(&copied).unwrap(),
            "example attribution text"
        );
        assert_eq!(record["sha256"], sha(&copied).unwrap());
        fs::remove_file(standard).unwrap();
        item["license"] = Value::Null;
        metadata["packages"][0] = item;
        let incomplete = collect(
            &metadata,
            &ids,
            &lock,
            &root.join("missing"),
            &sysroot,
            &json!("rustc"),
        )
        .unwrap();
        assert_eq!(incomplete["attribution_inputs_complete"], false);
        assert_eq!(incomplete["review_items"].as_array().unwrap().len(), 2);
        assert!(
            collect(
                &metadata,
                &BTreeSet::new(),
                &lock,
                &root.join("empty"),
                &sysroot,
                &json!("rustc")
            )
            .is_err()
        );
        scratch.close().unwrap();
    }
    #[test]
    fn declared_paths_and_symlinks_cannot_escape() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "notice-paths").unwrap();
        let root = scratch.path();
        let package = root.join("package");
        fs::create_dir(&package).unwrap();
        let outside = root.join("secret");
        fs::write(&outside, "must not be copied").unwrap();
        for file in [
            "../secret".into(),
            outside.to_string_lossy().into_owned(),
            "missing.txt".into(),
        ] {
            let item = json!({"manifest_path":package.join("Cargo.toml"),"source":"registry+example","license_file":file});
            let (paths, problems) =
                candidates(&item, &root.join("workspace"), &root.join("output")).unwrap();
            assert!(paths.is_empty());
            assert_eq!(problems.len(), 1);
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, package.join("LICENSE")).unwrap();
            let(paths,problems)=candidates(&json!({"manifest_path":package.join("Cargo.toml"),"source":"registry+example","license_file":null}),&root.join("workspace"),&root.join("output")).unwrap();
            assert!(paths.is_empty());
            assert_eq!(problems.len(), 1);
        }
        scratch.close().unwrap();
    }
    #[test]
    fn notice_copy_ceiling_is_enforced_before_copy() {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "notice-budget").unwrap();
        let file = scratch.path().join("LICENSE");
        fs::File::create(&file)
            .unwrap()
            .set_len(MAX_NOTICE + 1)
            .unwrap();
        let target = scratch.path().join("output");
        fs::create_dir(&target).unwrap();
        let mut copies = Copies {
            destination: &target,
            total: 0,
        };
        assert!(copies.copy(&file, Path::new("large")).is_err());
        assert!(!target.join("large").exists());
        fs::write(&file, b"xx").unwrap();
        copies.total = MAX_TOTAL - 1;
        assert!(copies.copy(&file, Path::new("total")).is_err());
        assert!(!target.join("total").exists());
        scratch.close().unwrap();
    }
}
