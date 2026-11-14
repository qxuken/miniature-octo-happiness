#![cfg(any(target_os = "macos", target_os = "linux", windows))]

use std::{fs, process::Command};

#[test]
fn invalid_admin_commands_do_not_touch_paths_and_errors_do_not_print_records() {
    let directory =
        std::env::temp_dir().join(format!("weekend-admin-decisions-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let directory = Cleanup(directory);
    let source = directory.0.join("private-source");
    let destination = directory.0.join("destination");
    fs::write(&source, b"PRIVATE_RECORD_CONTENT").unwrap();
    fs::write(&destination, b"existing destination").unwrap();
    for command in [
        "unknown", "check", "backup", "restore", "salvage", "export", "import",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_weekend-admin"))
            .arg(command)
            .arg(&source)
            .arg(&destination)
            .arg("excess argument")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{command}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("Invalid"));
        assert!(!error.contains("PRIVATE_RECORD_CONTENT"));
        assert_eq!(fs::read(&source).unwrap(), b"PRIVATE_RECORD_CONTENT");
        assert_eq!(fs::read(&destination).unwrap(), b"existing destination");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_weekend-admin"))
        .arg("check")
        .arg(&source)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(!error.contains("PRIVATE_RECORD_CONTENT"));
    assert!(!error.contains(&*source.to_string_lossy()));
    assert_eq!(fs::read(&source).unwrap(), b"PRIVATE_RECORD_CONTENT");
}
