//! Actual downstream rustc compilation, beyond expansion-function diagnostics.
use std::{fs, process::Command};

#[test]
fn malformed_attributes_fail_in_downstream_compilation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("weekend-macro-ui-{}", std::process::id()));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("Cargo.toml"), format!(
        "[package]\nname='macro-ui'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\nweekend-db-macros={{path={:?}}}\n", root)).unwrap();
    let cases = [
        (
            "Model",
            "#[weekend(type_id=1,version=0)] struct Bad { #[weekend(id=1)] a:u64 }",
            "nonzero",
        ),
        (
            "Model",
            "#[weekend(type_id=1,type_id=2,version=1)] struct Bad {}",
            "duplicate type_id",
        ),
        (
            "Model",
            "#[weekend(type_id=1,version=1)] struct Bad { #[weekend(id=1)] a:u64, #[weekend(id=1)] b:u64 }",
            "duplicates",
        ),
        (
            "Model",
            "#[weekend(type_id=1,version=1,reserved(1))] struct Bad { #[weekend(id=1)] a:u64 }",
            "reserved",
        ),
        (
            "Model",
            "#[weekend(type_id=1,version=1)] struct Bad { #[weekend(id=1,uniqe)] a:u64 }",
            "expected id",
        ),
        (
            "Model",
            "#[weekend(type_id=1,version=1)] struct Bad<T> { #[weekend(id=1)] a:T }",
            "nongeneric",
        ),
        (
            "Record",
            "#[record(key=1,format=b\"12345678\",fields(a,a))] struct Bad { a:u64 }",
            "duplicate",
        ),
        (
            "Record",
            "#[record(key=1,format=b\"12345678\",fields(unknown))] struct Bad { a:u64 }",
            "unknown",
        ),
        (
            "Record",
            "#[record(key=1,format=b\"12345678\",fields())] struct Bad { a:u64 }",
            "field",
        ),
        (
            "Resolution",
            "#[resolve] struct Bad { a:u64 }",
            "put #[resolve] on a field",
        ),
        (
            "Resolution",
            "#[resolution(unknown)] struct Bad { a:u64 }",
            "expected crate",
        ),
        (
            "Resolution",
            "struct Bad { #[resolve] #[resolve] a:u64 }",
            "duplicate",
        ),
    ];
    for (derive, source, expected) in cases {
        fs::write(
            dir.join("src/lib.rs"),
            format!("#[derive(weekend_db_macros::{derive})]\n{source}\n"),
        )
        .unwrap();
        let result = Command::new(env!("CARGO"))
            .args(["check", "--offline", "--quiet"])
            .arg("--manifest-path")
            .arg(dir.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", dir.join("target"))
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success(), "unexpected success for {source}");
        assert!(
            stderr.contains(expected),
            "expected {expected:?} for {source}: {stderr}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
