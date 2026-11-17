use weekend_db_typed::{Result, Store};
use weekend_tasks::Tasks;
#[test]
fn task_ids_survive_failed_inserts_deletion_and_undo() -> Result<()> {
    let mut store = Store::memory()?;
    let tasks = Tasks::attach(&mut store)?;
    assert_eq!(tasks.add(&store, "one".into(), "First".into())?, 1);
    let checkpoint = store.checkpoint()?;
    assert!(tasks.add(&store, "one".into(), "Duplicate".into()).is_err());
    assert_eq!(tasks.add(&store, "two".into(), "Second".into())?, 2);
    tasks.remove(&store, "two")?;
    let backup = tasks.undo(&store, checkpoint)?;
    assert!(backup > checkpoint);
    assert_eq!(tasks.add(&store, "three".into(), "Third".into())?, 3);
    Ok(())
}
#[test]
fn portable_demo_runs() -> Result<()> {
    weekend_tasks::demo()
}

#[cfg(any(
    target_os = "macos",
    windows,
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
))]
#[test]
fn cli_reopens_history_and_restores_real_files() {
    use std::{fs, process::Command};
    let directory = std::env::temp_dir().join(format!("weekend-task-cli-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let cleanup = Cleanup(directory);
    let path = cleanup.0.join("tasks.wdb");
    let run = |args: &[&str], success: bool| {
        let out = Command::new(env!("CARGO_BIN_EXE_weekend-tasks"))
            .arg("--db")
            .arg(&path)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            success,
            "args={args:?}, stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    run(&["init"], true);
    run(&["init"], false);
    assert!(run(&["add", "first", "Build a Rust app"], true).contains("#1"));
    run(&["add", "first", "Duplicate"], false);
    assert!(run(&["add", "second", "Read the docs"], true).contains("#2"));
    let checkpoint = run(&["checkpoint"], true);
    let checkpoint = checkpoint.trim();
    run(&["done", "first"], true);
    run(&["tag", "first", "rust"], true);
    run(&["note", "first", "Keep unknown fields"], true);
    run(&["priority", "first", "3"], true);
    run(&["rename", "first", "second"], false);
    assert!(run(&["show", "first"], true).contains("[x]"));
    assert!(run(&["list", "--snapshot", checkpoint], true).contains("[ ]"));
    run(&["undo", checkpoint], true);
    assert!(run(&["show", "first"], true).contains("[ ]"));
    let revision = {
        let store = Store::open_file(&path).unwrap();
        store.database().snapshot().revision()
    };
    run(&["list"], true);
    {
        let store = Store::open_file(&path).unwrap();
        assert_eq!(store.database().snapshot().revision(), revision);
    }
    run(&["compact"], !cfg!(windows));
    assert!(run(&["show", "first"], true).contains("Build a Rust app"));
    assert!(run(&["history"], true).contains(checkpoint));
    run(&["upgrade"], true);
    run(&["forget", checkpoint], true);
}
