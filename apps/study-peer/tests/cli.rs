use std::process::Command;
#[test]
fn cli_dispatch_rejects_invalid_shapes_and_accepts_help() {
    let bin = env!("CARGO_BIN_EXE_study-peer");
    for args in [vec!["--help"], vec!["-h"]] {
        assert!(
            Command::new(bin)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    for args in [
        vec![],
        vec!["unknown"],
        vec!["wrong", "unused"],
        vec!["wrong", "p", "a", "11111111111111111111111111111111"],
        vec!["wrong", "p", "a", "id", "l", "r", "c", "pin"],
        vec!["run", "p", "a", "id", "l", "r", "c", "pin", "--bad"],
        vec!["init", "p", "c", "11111111111111111111111111111111"],
    ] {
        assert!(
            !Command::new(bin)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
