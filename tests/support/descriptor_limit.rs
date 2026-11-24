use std::{fs::File, path::Path, process::Command};

pub(crate) const EMFILE: i32 = 24;

pub(crate) fn fill_table() -> Vec<File> {
    let mut descriptors = Vec::new();
    for _ in 0..64 {
        match File::open("/dev/null") {
            Ok(file) => descriptors.push(file),
            Err(error) => {
                assert_eq!(error.raw_os_error(), Some(EMFILE));
                return descriptors;
            }
        }
    }
    panic!("descriptor limit was not enforced");
}

pub(crate) fn run_worker(test: &str, setting: (&str, &str), cwd: &Path, receipt: &str) {
    let output = Command::new("/bin/sh")
        .args(["-c", "ulimit -Sn 64 && exec \"$@\"", "descriptor-worker"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(setting.0, setting.1)
        .current_dir(cwd)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains(receipt),
        "{receipt}: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{receipt}");
}
