use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const CATALOG: &str = "docs/decisions/workspaces.json";

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = env::temp_dir().join(format!(
            "decision-catalog-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let f = Self(root.canonicalize().unwrap());
        f.package(
            "",
            "fixture-root",
            "pub fn root(flag:bool){if flag {}}",
            true,
        );
        f.package(
            "other/",
            "fixture-standalone",
            "fn main(){if false {}}",
            false,
        );
        f.write(
            "build.rs",
            "fn main(){panic!(\"metadata must not execute build scripts\");}",
        );
        f.write("examples/showcase.rs", "fn main(){if true {}}");
        f.write(
            "tests/extra.rs",
            "#[test] fn integration_branch(){if true {}}",
        );
        f.write(
            "other/src/bin/zero.rs",
            "macro_rules! opaque {()=>{if true {}}} fn main(){opaque!();}",
        );
        f.catalog(&["Cargo.toml", "other/Cargo.toml"]);
        f
    }
    fn write(&self, name: &str, text: &str) {
        let file = self.0.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    fn package(&self, prefix: &str, name: &str, code: &str, lib: bool) {
        self.write(
            &format!("{prefix}Cargo.toml"),
            &format!(
                "[package]\nname=\"{name}\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n"
            ),
        );
        self.write(
            &format!("{prefix}Cargo.lock"),
            &format!("version=4\n[[package]]\nname=\"{name}\"\nversion=\"0.1.0\"\n"),
        );
        self.write(
            &format!("{prefix}src/{}.rs", if lib { "lib" } else { "main" }),
            code,
        );
    }
    fn catalog(&self, manifests: &[&str]) {
        self.write(
            CATALOG,
            &serde_json::to_string_pretty(&json!({"schema_version":1,"manifests":manifests}))
                .unwrap(),
        );
    }
    fn run(&self, output: &str, tests: bool) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_weekend-decision-inventory"));
        c.arg(&self.0).arg(self.0.join(output));
        if tests {
            c.arg("--include-tests");
        }
        c.output().unwrap()
    }
    fn report(&self, output: &str, tests: bool) -> Value {
        let r = self.run(output, tests);
        assert!(
            r.status.success(),
            "inventory failed: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        serde_json::from_slice(&fs::read(self.0.join(output).join("inventory.json")).unwrap())
            .unwrap()
    }
    fn git(&self, args: &[&str]) {
        let r = Command::new("git")
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap();
        assert!(r.status.success(), "git fixture: {r:?}");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn target<'a>(report: &'a Value, entry: &str) -> &'a Value {
    report["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["entry"] == entry)
        .unwrap_or_else(|| panic!("missing Cargo target {entry}"))
}

#[test]
fn catalog_includes_standalones_examples_build_scripts_and_zero_record_targets() {
    let f = Fixture::new();
    let r = f.report("output", false);
    assert_eq!(r["cargo_scope"]["mode"], "repository_catalog");
    assert_eq!(r["cargo_scope"]["catalog"]["file"], CATALOG);
    assert_eq!(r["cargo_scope"]["workspaces"].as_array().unwrap().len(), 2);
    assert_eq!(
        r["cargo_scope"]["manifests_sha256"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(r["targets"].as_array().unwrap().len(), 5);
    assert_eq!(
        target(&r, "src/lib.rs")["target"],
        "fixture-root::fixture_root"
    );
    assert_eq!(
        target(&r, "examples/showcase.rs")["kinds"],
        json!(["example"])
    );
    assert_eq!(target(&r, "build.rs")["kinds"], json!(["custom-build"]));
    target(&r, "other/src/main.rs");
    let zero = target(&r, "other/src/bin/zero.rs")["target"]
        .as_str()
        .unwrap();
    assert!(
        !r["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["target"] == zero)
    );
    assert!(
        r["blind_spots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["target"] == zero && d["kind"] == "unexpanded_macro")
    );
    assert!(
        r["cargo_scope"]["excluded_targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["entry"] == "tests/extra.rs")
    );
    assert!(
        !f.0.join("target").exists(),
        "inventory compiled inspected code"
    );
}

#[test]
fn optional_test_mode_adds_integration_entries_without_dropping_production_targets() {
    let f = Fixture::new();
    let r = f.report("with-tests", true);
    assert_eq!(target(&r, "tests/extra.rs")["kinds"], json!(["test"]));
    assert!(
        r["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["owner"] == "integration_branch")
    );
    target(&r, "other/src/main.rs");
    target(&r, "src/lib.rs");
}

#[test]
fn catalog_rejects_missing_unlisted_duplicate_and_escaping_manifests() {
    for (name, manifests, marker) in [
        (
            "missing",
            vec!["Cargo.toml", "absent/Cargo.toml"],
            "manifest",
        ),
        ("unlisted", vec!["Cargo.toml"], "manifest scope"),
        (
            "duplicate",
            vec!["Cargo.toml", "Cargo.toml", "other/Cargo.toml"],
            "duplicate",
        ),
        ("escape", vec!["Cargo.toml", "../Cargo.toml"], "relative"),
    ] {
        let f = Fixture::new();
        f.catalog(&manifests);
        let before = fs::read(f.0.join("Cargo.lock")).unwrap();
        let r = f.run(name, false);
        assert!(!r.status.success(), "invalid catalog accepted: {name}");
        assert!(
            String::from_utf8_lossy(&r.stderr).contains(marker),
            "wrong rejection {name}: {r:?}"
        );
        assert!(!f.0.join(name).exists());
        assert_eq!(fs::read(f.0.join("Cargo.lock")).unwrap(), before);
    }
}

#[test]
fn tracked_manifest_additions_and_broken_git_metadata_do_not_silently_shrink_scope() {
    let f = Fixture::new();
    f.git(&["init", "-q"]);
    f.git(&["add", "."]);
    let good = f.report("tracked", false);
    assert_eq!(good["cargo_scope"]["manifest_basis"], "git-tracked");
    f.package("new/", "new-member", "fn main(){}", false);
    f.git(&["add", "new"]);
    let r = f.run("new-output", false);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("manifest scope"));
    fs::remove_file(f.0.join(".git/HEAD")).unwrap();
    let r = f.run("broken-output", false);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("git"));
}

#[test]
fn workspace_and_target_kind_disambiguate_identical_package_target_names() {
    let f = Fixture::new();
    f.package(
        "other/",
        "fixture-root",
        "pub fn other(){if false {}}",
        true,
    );
    // Preserve a valid independently named binary in that workspace too.
    f.write("other/src/main.rs", "fn main(){}");
    let mut manifest = fs::read_to_string(f.0.join("Cargo.toml")).unwrap();
    manifest.push_str("\n[[bin]]\nname=\"fixture_root\"\npath=\"src/bin.rs\"\n");
    f.write("Cargo.toml", &manifest);
    f.write("src/bin.rs", "fn main(){if true {}}");
    let r = f.report("collisions", false);
    let first = target(&r, "src/lib.rs");
    let second = target(&r, "other/src/lib.rs");
    let third = target(&r, "src/bin.rs");
    assert_ne!(first["target"], second["target"]);
    assert_ne!(first["target"], third["target"]);
    assert_ne!(second["target"], third["target"]);
    assert_eq!(first["workspace_manifest"], "Cargo.toml");
    assert_eq!(second["workspace_manifest"], "other/Cargo.toml");
}

#[test]
fn absent_catalog_retains_an_explicit_root_only_inventory() {
    let f = Fixture::new();
    fs::remove_file(f.0.join(CATALOG)).unwrap();
    let r = f.report("root-only", false);
    assert_eq!(r["cargo_scope"]["mode"], "root_workspace");
    assert!(
        !r["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| Path::new(t["entry"].as_str().unwrap()).starts_with("other"))
    );
    assert_eq!(
        target(&r, "src/lib.rs")["target"],
        "fixture-root::fixture_root"
    );
}

#[test]
fn workspace_members_share_one_metadata_scope_and_preserve_all_lock_bytes() {
    let f = Fixture::new();
    f.package(
        "member/",
        "fixture-member",
        "pub fn member(){if false {}}",
        true,
    );
    f.write(
        "member/Cargo.toml",
        "[package]\nname='fixture-member'\nversion='0.1.0'\nedition='2024'\n",
    );
    fs::remove_file(f.0.join("member/Cargo.lock")).unwrap();
    f.write("Cargo.toml", "[package]\nname='fixture-root'\nversion='0.1.0'\nedition='2024'\n[workspace]\nmembers=['member']\n");
    f.write("Cargo.lock", "version=4\n[[package]]\nname='fixture-root'\nversion='0.1.0'\n[[package]]\nname='fixture-member'\nversion='0.1.0'\n");
    f.catalog(&["Cargo.toml", "member/Cargo.toml", "other/Cargo.toml"]);
    let locks = ["Cargo.lock", "other/Cargo.lock"].map(|p| (p, fs::read(f.0.join(p)).unwrap()));
    let r = f.report("members", false);
    assert_eq!(r["cargo_scope"]["workspaces"].as_array().unwrap().len(), 2);
    assert_eq!(
        r["cargo_scope"]["manifests_sha256"]
            .as_object()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        r["cargo_scope"]["lockfiles_sha256"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    target(&r, "member/src/lib.rs");
    for (path, before) in locks {
        assert_eq!(fs::read(f.0.join(path)).unwrap(), before);
    }
    assert!(!f.0.join("member/Cargo.lock").exists());
    assert!(!f.0.join("target").exists());
}

#[test]
fn missing_workspace_lock_rejects_without_creating_it_or_building() {
    let f = Fixture::new();
    fs::remove_file(f.0.join("other/Cargo.lock")).unwrap();
    let r = f.run("missing-lock", false);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("lock"));
    assert!(!f.0.join("other/Cargo.lock").exists());
    assert!(!f.0.join("missing-lock").exists());
    assert!(!f.0.join("target").exists());
}

#[cfg(unix)]
#[test]
fn catalog_and_lock_aliases_cannot_hide_inputs() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    fs::rename(
        f.0.join("other/Cargo.lock"),
        f.0.join("other/original.lock"),
    )
    .unwrap();
    symlink("original.lock", f.0.join("other/Cargo.lock")).unwrap();
    assert!(!f.run("lock-alias", false).status.success());
    let f = Fixture::new();
    fs::rename(f.0.join(CATALOG), f.0.join("original-catalog.json")).unwrap();
    symlink("../../original-catalog.json", f.0.join(CATALOG)).unwrap();
    assert!(!f.run("catalog-alias", false).status.success());
}

#[test]
fn benches_keep_their_own_target_identity_without_being_compiled() {
    let f = Fixture::new();
    f.write("benches/timing.rs", "fn main(){if true {}}\n");
    let r = f.report("bench-output", false);
    let bench = target(&r, "benches/timing.rs");
    assert_eq!(bench["kinds"], json!(["bench"]));
    assert!(
        r["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["target"] == bench["target"])
    );
    assert!(!f.0.join("target").exists());
}

#[test]
fn source_archive_inside_another_git_tree_uses_its_own_filesystem_scope() {
    let parent = Fixture::new();
    parent.git(&["init", "-q"]);
    let archive = parent.0.join("archive");
    fs::create_dir(&archive).unwrap();
    for entry in [
        "Cargo.toml",
        "Cargo.lock",
        "src",
        "other",
        "docs",
        "build.rs",
        "examples",
        "tests",
    ] {
        fs::rename(parent.0.join(entry), archive.join(entry)).unwrap();
    }
    let f = Fixture(archive.canonicalize().unwrap());
    let r = f.report("archive-output", false);
    assert_eq!(r["cargo_scope"]["manifest_basis"], "filesystem");
    assert_eq!(r["cargo_scope"]["workspaces"].as_array().unwrap().len(), 2);
}

#[test]
fn git_root_paths_preserve_spaces_including_unix_trailing_spaces() {
    let suffixes = vec![" with spaces"];
    #[cfg(unix)]
    let suffixes = suffixes
        .into_iter()
        .chain([" with trailing space "])
        .collect::<Vec<_>>();
    for suffix in suffixes {
        let mut f = Fixture::new();
        let mut renamed = f.0.clone();
        renamed.set_file_name(format!(
            "{}{suffix}",
            f.0.file_name().unwrap().to_str().unwrap()
        ));
        fs::rename(&f.0, &renamed).unwrap();
        f.0 = renamed.canonicalize().unwrap();
        f.git(&["init", "-q"]);
        f.git(&["add", "."]);
        let r = f.report("space-output", false);
        assert_eq!(r["cargo_scope"]["manifest_basis"], "git-tracked");
    }
}
