//! Actual downstream rustc compilation, beyond expansion-function diagnostics.
//!
//! The test process still runs under the parent suite's sanitizer. Nested Cargo
//! checks only assert compile-time diagnostics and use an uninstrumented host
//! build: otherwise global sanitizer flags also instrument the host proc macro,
//! which rustc rejects before it can diagnose the deliberately malformed input.
use std::{fs, process::Command};

fn diagnostic_environment(command: &mut Command) {
    // Include command-local overrides so regression tests need no process-wide
    // environment mutation (which would race with parallel tests).
    let keys = std::env::vars_os()
        .map(|(key, _)| key)
        .chain(command.get_envs().map(|(key, _)| key.to_owned()))
        .collect::<Vec<_>>();
    for key in keys {
        let name = key.to_string_lossy();
        if matches!(
            name.as_ref(),
            "RUSTFLAGS"
                | "RUSTDOCFLAGS"
                | "CARGO_ENCODED_RUSTFLAGS"
                | "CARGO_ENCODED_RUSTDOCFLAGS"
                | "CARGO_BUILD_RUSTFLAGS"
                | "CARGO_BUILD_RUSTDOCFLAGS"
                | "CARGO_BUILD_TARGET"
                | "CARGO_UNSTABLE_BUILD_STD"
                | "CARGO_UNSTABLE_BUILD_STD_FEATURES"
        ) || (name.starts_with("CARGO_TARGET_") && name.ends_with("_RUSTFLAGS"))
        {
            command.env_remove(key);
        }
    }
}

#[test]
fn diagnostic_commands_remove_inherited_instrumentation_and_target_selection() {
    let mut command = Command::new(env!("CARGO"));
    let names = [
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_ENCODED_RUSTDOCFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_BUILD_RUSTDOCFLAGS",
        "CARGO_BUILD_TARGET",
        "CARGO_UNSTABLE_BUILD_STD",
        "CARGO_UNSTABLE_BUILD_STD_FEATURES",
        "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
    ];
    for name in names {
        command.env(name, "deliberately-invalid-instrumentation-or-target");
    }
    command.env("RUSTUP_TOOLCHAIN", "preserve-toolchain");
    diagnostic_environment(&mut command);
    for name in names {
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == name && value.is_none())
        );
    }
    assert!(
        command
            .get_envs()
            .any(|(key, value)| key == "RUSTUP_TOOLCHAIN"
                && value == Some(std::ffi::OsStr::new("preserve-toolchain")))
    );
}

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
        let mut command = Command::new(env!("CARGO"));
        command
            .args(["check", "--offline", "--quiet"])
            .arg("--manifest-path")
            .arg(dir.join("Cargo.toml"))
            .current_dir(&dir)
            .env("CARGO_TARGET_DIR", dir.join("target"));
        diagnostic_environment(&mut command);
        let result = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success(), "unexpected success for {source}");
        assert!(
            stderr.contains(expected),
            "expected {expected:?} for {source}: {stderr}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
