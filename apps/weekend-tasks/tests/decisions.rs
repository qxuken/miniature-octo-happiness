use weekend_db_typed::{Error, Store, engine};
use weekend_tasks::{LegacyTask, Tasks};

#[test]
fn attach_recovers_next_id_and_failed_actions_preserve_tasks() {
    let mut store = Store::memory().unwrap();
    let legacy = store.table::<LegacyTask>("tasks").unwrap();
    store
        .write(|w| {
            w.insert(
                &legacy,
                41,
                &LegacyTask {
                    slug: "old".into(),
                    title: "Before the counter existed".into(),
                    done: false,
                    tags: vec![],
                },
            )
        })
        .unwrap();
    let tasks = Tasks::attach(&mut store).unwrap();
    let revision = store.database().snapshot().revision();
    let tasks_again = Tasks::attach(&mut store).unwrap();
    assert_eq!(store.database().snapshot().revision(), revision);
    for (slug, title) in [(" ", "title"), ("slug", "\t")] {
        assert!(matches!(
            tasks.add(&store, slug.into(), title.into()),
            Err(Error::Schema(engine::schema::Error::InvalidValue))
        ));
        assert_eq!(store.database().snapshot().revision(), revision);
    }
    assert_eq!(tasks_again.add(&store, "new".into(), "New".into()), Ok(42));
    let revision = store.database().snapshot().revision();
    assert_eq!(
        tasks.update(&store, "missing", |_| panic!("callback must not run")),
        Err(Error::Engine(engine::Error::NotFound))
    );
    assert_eq!(
        tasks.remove(&store, "missing"),
        Err(Error::Engine(engine::Error::NotFound))
    );
    assert_eq!(
        tasks.update(&store, "new", |task| {
            task.title = "discard me".into();
            Err(Error::Engine(engine::Error::Invalid))
        }),
        Err(Error::Engine(engine::Error::Invalid))
    );
    assert!(tasks.undo(&store, 999).is_err());
    assert_eq!(store.database().snapshot().revision(), revision);
    assert!(store.history().unwrap().is_empty());
    assert_eq!(
        store
            .read(|r| Ok(r.get(&tasks.table, 42)?.unwrap().title.clone()))
            .unwrap(),
        "New"
    );
}

#[test]
fn counter_initialization_and_increment_report_exhaustion() {
    for id in [u64::MAX - 1, u64::MAX] {
        let mut store = Store::memory().unwrap();
        let legacy = store.table::<LegacyTask>("tasks").unwrap();
        store
            .write(|w| {
                w.insert(
                    &legacy,
                    id,
                    &LegacyTask {
                        slug: "last".into(),
                        title: "Last ID".into(),
                        done: false,
                        tags: vec![],
                    },
                )
            })
            .unwrap();
        if id == u64::MAX {
            // Schema registration can commit before initialization fails.
            assert!(matches!(
                Tasks::attach(&mut store),
                Err(Error::Engine(engine::Error::Exhausted))
            ));
        } else {
            let tasks = Tasks::attach(&mut store).unwrap();
            let revision = store.database().snapshot().revision();
            assert_eq!(
                tasks.add(&store, "overflow".into(), "Overflow".into()),
                Err(Error::Engine(engine::Error::Exhausted))
            );
            assert_eq!(store.database().snapshot().revision(), revision);
        }
        assert_eq!(store.read(|r| Ok(r.list(&legacy)?.len())).unwrap(), 1);
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod native {
    use super::*;
    use std::{ffi::OsStr, fs, path::PathBuf, process::Command};

    struct Directory(PathBuf);
    impl Directory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "weekend-task-decisions-{label}-{}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn cli(args: &[&OsStr], success: bool) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_weekend-tasks"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn cli_admission_rejects_arguments_before_opening_a_file() {
        let directory = Directory::new("admission");
        let path = directory.0.join("untouched.wdb");
        fs::write(&path, b"sentinel").unwrap();
        for args in [
            vec!["unknown"],
            vec!["init", "extra"],
            vec!["show"],
            vec!["add", "slug"],
            vec!["list", "--wrong", "1"],
            vec!["list", "--snapshot"],
            vec!["demo", "extra"],
            vec!["compact", "extra"],
        ] {
            let mut command = vec![OsStr::new("--db"), path.as_os_str()];
            command.extend(args.iter().map(OsStr::new));
            cli(&command, false);
            assert_eq!(fs::read(&path).unwrap(), b"sentinel");
        }
        cli(&[OsStr::new("--db")], false);
        for command in ["help", "--help", "-h"] {
            assert!(cli(&[OsStr::new(command)], true).contains("weekend-tasks"));
        }
        assert!(cli(&[], true).contains("weekend-tasks"));
        assert!(cli(&[OsStr::new("demo")], true).contains("Task demo passed"));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            cli(&[OsStr::from_bytes(b"\xff")], false);
            cli(&[OsStr::new("add"), OsStr::from_bytes(b"\xff")], false);
        }
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn cli_edit_branches_preserve_failed_actions_and_deduplicate_tags() {
        let directory = Directory::new("edits");
        let path = directory.0.join("tasks.wdb");
        let run = |args: &[&str], success| {
            let mut command = vec![OsStr::new("--db"), path.as_os_str()];
            command.extend(args.iter().map(OsStr::new));
            cli(&command, success)
        };
        run(&["init"], true);
        run(&["add", "a", "A"], true);
        let revision = || {
            Store::open_file(&path)
                .unwrap()
                .database()
                .snapshot()
                .revision()
        };
        let before = revision();
        for args in [
            vec!["add", " ", "A"],
            vec!["add", "b", " "],
            vec!["title", "a", " "],
            vec!["rename", "a", " "],
            vec!["tag", "a", " "],
            vec!["priority", "a", "6"],
            vec!["priority", "a", "256"],
            vec!["priority", "a", "x"],
            vec!["show", "missing"],
            vec!["remove", "missing"],
            vec!["undo", "999"],
            vec!["undo", "x"],
            vec!["forget", "999"],
            vec!["list", "--snapshot", "x"],
        ] {
            run(&args, false);
            assert_eq!(revision(), before, "{args:?}");
        }
        run(&["done", "a"], true);
        run(&["reopen", "a"], true);
        run(&["title", "a", "Renamed title"], true);
        run(&["rename", "a", "b"], true);
        run(&["tag", "b", "rust"], true);
        run(&["tag", "b", "rust"], true);
        run(&["note", "b", "temporary"], true);
        run(&["note", "b", "--clear"], true);
        run(&["priority", "b", "5"], true);
        let output = run(&["show", "b"], true);
        assert!(output.contains("[ ]\tb\tp5\tRenamed title"));
        assert!(output.contains("tags: rust\n"));
        assert!(!output.contains("note:"));
        run(&["remove", "b"], true);
        assert!(run(&["list"], true).is_empty());
    }

    #[test]
    fn cli_requires_explicit_upgrade_before_editing_old_records() {
        let directory = Directory::new("upgrade");
        let path = directory.0.join("tasks.wdb");
        let revision = {
            let mut store = Store::create_file(&path).unwrap();
            let legacy = store.table::<LegacyTask>("tasks").unwrap();
            store
                .write(|w| {
                    w.insert(
                        &legacy,
                        1,
                        &LegacyTask {
                            slug: "old".into(),
                            title: "Legacy task".into(),
                            done: false,
                            tags: vec![],
                        },
                    )
                })
                .unwrap();
            Tasks::attach(&mut store).unwrap();
            store.database().snapshot().revision()
        };
        let output = Command::new(env!("CARGO_BIN_EXE_weekend-tasks"))
            .arg("--db")
            .arg(&path)
            .args(["done", "old"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("migrate older tasks explicitly")
        );
        assert_eq!(
            Store::open_file(&path)
                .unwrap()
                .database()
                .snapshot()
                .revision(),
            revision
        );
        let mut command = vec![OsStr::new("--db"), path.as_os_str(), OsStr::new("upgrade")];
        assert!(cli(&command, true).contains("Migrated 1 tasks"));
        command[2] = OsStr::new("done");
        command.push(OsStr::new("old"));
        cli(&command, true);
        command[2] = OsStr::new("show");
        assert!(cli(&command, true).contains("[x]"));
    }
}
