#![cfg(any(target_os = "macos", target_os = "linux", windows))]
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_weekend-replicate");
#[test]
fn cli_help_and_wrong_dispatch_shapes_have_controls() {
    for help in ["--help", "-h"] {
        assert!(
            Command::new(BIN)
                .arg(help)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    for args in [
        vec![],
        vec!["unknown"],
        vec!["wrong", "path"],
        vec!["wrong", "source", "stream"],
        vec!["wrong", "dir", "stream", "identity"],
        vec!["wrong", "source", "stream", "address", "credentials", "pin"],
        vec![
            "wrong",
            "dir",
            "stream",
            "identity",
            "address",
            "credentials",
            "pin",
        ],
    ] {
        let output = Command::new(BIN).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("weekend-replicate keygen"));
    }
}
