//! SQLite side of the private durable retained-history comparison.
mod workload;

use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
    params,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::Instant,
};
use workload::{Expected, State, Work};

const SETTINGS: [(&str, u64); 5] = [
    ("synchronous", 2),
    ("fullfsync", 1),
    ("checkpoint_fullfsync", 1),
    ("wal_autocheckpoint", 1000),
    ("busy_timeout", 5000),
];
const SAMPLES: [&str; 5] = [
    "commit_ns",
    "transaction_ns",
    "pin_ns",
    "reopen_ns",
    "reopen_verify_ns",
];

fn connect(path: &Path, create: bool) -> Result<Connection> {
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let db = Connection::open_with_flags(path, flags)?;
    if create {
        db.pragma_update(None, "page_size", 4096)?;
    }
    let journal = db.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))?;
    ensure!(journal == "wal", "WAL mode rejected");
    for (name, value) in SETTINGS
        .into_iter()
        .map(|(name, value)| (name, value as i64))
        .chain([("cache_size", -2000), ("mmap_size", 0)])
    {
        db.pragma_update(None, name, value)?;
        ensure!(
            db.pragma_query_value(None, name, |row| row.get::<_, i64>(0))? == value,
            "{name}"
        );
    }
    ensure!(
        db.pragma_query_value(None, "page_size", |row| row.get::<_, u64>(0))? == 4096,
        "page size mismatch"
    );
    Ok(db)
}

fn close(db: Connection) -> Result<()> {
    db.close()
        .map_err(|(_, error)| error)
        .context("close SQLite")
}

fn items<T>(
    db: &Connection,
    sql: &str,
    read: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    Ok(db
        .prepare(sql)?
        .query_map([], read)?
        .collect::<rusqlite::Result<_>>()?)
}

fn sizes(path: &Path) -> Result<Value> {
    let mut sizes = serde_json::Map::new();
    for (key, suffix) in [("main", ""), ("-wal", "-wal"), ("-shm", "-shm")] {
        let mut entry = path.as_os_str().to_owned();
        entry.push(suffix);
        let len = match fs::metadata(entry) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        sizes.insert(key.into(), json!(len));
    }
    Ok(Value::Object(sizes))
}

fn read_state(db: &Connection, epoch: u64) -> Result<State> {
    let mut statement = db.prepare(
        "SELECT v.k,v.payload FROM versions v JOIN
        (SELECT k,MAX(epoch) AS epoch FROM versions WHERE epoch<=? GROUP BY k) x
        USING(k,epoch)",
    )?;
    let mut rows = statement.query([epoch])?;
    let mut state = State::new();
    while let Some(row) = rows.next()? {
        let key = row
            .get_ref(0)?
            .as_blob()
            .context("noncanonical binary key")?;
        let key = u64::from_le_bytes(key.try_into().ok().context("noncanonical binary key")?);
        ensure!(
            state.insert(key, row.get(1)?).is_none(),
            "duplicate logical key"
        );
    }
    Ok(state)
}

fn verify(db: &Connection, work: Work, expected: &Expected) -> Result<()> {
    ensure!(
        items(db, "SELECT epoch FROM head", |row| row.get::<_, u64>(0))? == [work.commits],
        "head mismatch"
    );
    let catalog = items(db, "SELECT id,epoch FROM retained ORDER BY id", |row| {
        Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?))
    })?;
    ensure!(
        catalog
            .into_iter()
            .eq((1..).zip(expected.retained.keys().copied())),
        "retained catalog mismatch"
    );
    ensure!(
        read_state(db, work.commits)? == expected.current,
        "current state mismatch"
    );
    for (epoch, state) in &expected.retained {
        ensure!(
            read_state(db, *epoch)? == *state,
            "retained state mismatch at {epoch}"
        );
    }
    Ok(())
}

fn verify_path(path: &Path, work: Work, expected: &Expected) -> Result<()> {
    let db = connect(path, false)?;
    ensure!(
        items(&db, "PRAGMA integrity_check", |row| row.get::<_, String>(0))? == ["ok"],
        "integrity check failed"
    );
    verify(&db, work, expected)?;
    close(db)
}

fn sync_output(path: &Path) -> Result<()> {
    let file = File::open(path)?;
    file.sync_all()?;
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        // This descriptor is owned by file for the whole call. F_FULLFSYNC
        // takes no additional arguments on macOS.
        let status = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) };
        if status == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    File::open(path.parent().context("SQLite output parent")?)?.sync_all()?;
    Ok(())
}

fn verified_copy(source: &Path, target: &Path, work: Work, expected: &Expected) -> Result<u128> {
    ensure!(
        fs::symlink_metadata(target)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "copy destination already exists"
    );
    let started = Instant::now();
    verify_path(source, work, expected)?;
    drop(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?,
    );
    let src = connect(source, false)?;
    let mut dst = Connection::open_with_flags(target, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    dst.pragma_update(None, "synchronous", 2)?;
    dst.pragma_update(None, "fullfsync", 1)?;
    {
        let backup = Backup::new(&src, &mut dst)?;
        ensure!(
            matches!(backup.step(-1)?, StepResult::Done),
            "SQLite backup incomplete"
        );
    }
    close(dst)?;
    close(src)?;
    verify_path(target, work, expected)?;
    sync_output(target)?;
    Ok(started.elapsed().as_nanos())
}

fn checkpoint(db: &Connection) -> Result<[u64; 3]> {
    let status = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok([row.get(0)?, row.get(1)?, row.get(2)?])
    })?;
    ensure!(status == [0, 0, 0], "checkpoint incomplete: {status:?}");
    Ok(status)
}

fn sqlite_run(directory: &Path, work: Work, expected: &Expected) -> Result<Value> {
    fs::create_dir(directory)?;
    let path = directory.join("main.sqlite");
    let db = connect(&path, true)?;
    let mut settings = serde_json::Map::new();
    settings.insert(
        "journal_mode".into(),
        json!(db.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))?),
    );
    for name in
        SETTINGS
            .map(|(name, _)| name)
            .into_iter()
            .chain(["page_size", "cache_size", "mmap_size"])
    {
        settings.insert(
            name.into(),
            json!(db.pragma_query_value(None, name, |row| row.get::<_, i64>(0))?),
        );
    }
    let options = items(&db, "PRAGMA compile_options", |row| row.get::<_, String>(0))?;
    db.execute_batch(
        "BEGIN IMMEDIATE;
        CREATE TABLE versions(k BLOB NOT NULL,epoch INTEGER NOT NULL,payload BLOB NOT NULL,
            PRIMARY KEY(k,epoch)) WITHOUT ROWID;
        CREATE TABLE retained(id INTEGER PRIMARY KEY,epoch INTEGER NOT NULL);
        CREATE TABLE head(epoch INTEGER NOT NULL);
        INSERT INTO head VALUES(0); COMMIT; BEGIN IMMEDIATE;",
    )?;
    {
        let mut seed = db.prepare("INSERT INTO versions VALUES(?,0,?)")?;
        for key in 0..work.keys {
            seed.execute(params![key.to_le_bytes(), work.payload(key, 0)])?;
        }
    }
    db.execute_batch("COMMIT")?;

    let [
        mut commit_ns,
        mut transaction_ns,
        mut pin_ns,
        mut reopen_ns,
        mut reopen_verify_ns,
    ] = SAMPLES.map(|_| Vec::new());
    for epoch in 1..=work.commits {
        let writes = (0..work.batch)
            .map(|offset| {
                let key = (epoch * work.batch + offset) % work.keys;
                (key.to_le_bytes(), work.payload(key, epoch))
            })
            .collect::<Vec<_>>();
        let started = Instant::now();
        db.execute_batch("BEGIN IMMEDIATE")?;
        {
            let mut insert = db.prepare("INSERT INTO versions VALUES(?,?,?)")?;
            for (key, data) in writes {
                insert.execute(params![key, epoch, data])?;
            }
        }
        db.execute("UPDATE head SET epoch=?", [epoch])?;
        let commit = Instant::now();
        db.execute_batch("COMMIT")?;
        commit_ns.push(commit.elapsed().as_nanos());
        transaction_ns.push(started.elapsed().as_nanos());
        if expected.retained.contains_key(&epoch) {
            let started = Instant::now();
            db.execute_batch("BEGIN IMMEDIATE")?;
            db.execute("INSERT INTO retained(epoch) VALUES(?)", [epoch])?;
            db.execute_batch("COMMIT")?;
            pin_ns.push(started.elapsed().as_nanos());
        }
    }
    verify(&db, work, expected)?;
    let version_rows = |db: &Connection| {
        db.query_row("SELECT COUNT(*) FROM versions", [], |row| {
            row.get::<_, u64>(0)
        })
    };
    let before_gc = version_rows(&db)?;
    ensure!(
        before_gc == work.keys + work.commits * work.batch,
        "history rows mismatch"
    );
    let sizes_live = sizes(&path)?;
    let started = Instant::now();
    let checkpoint_result = checkpoint(&db)?;
    let checkpoint_ns = started.elapsed().as_nanos();
    let sizes_checkpointed = sizes(&path)?;
    close(db)?;
    let sizes_closed = sizes(&path)?;

    for _ in 0..work.reopens {
        let started = Instant::now();
        let db = connect(&path, false)?;
        ensure!(
            items(&db, "SELECT epoch FROM head", |row| row.get::<_, u64>(0))? == [work.commits],
            "reopen head mismatch"
        );
        ensure!(
            items(&db, "SELECT id FROM retained", |row| row.get::<_, u64>(0))?.len()
                == expected.retained.len(),
            "reopen catalog mismatch"
        );
        reopen_ns.push(started.elapsed().as_nanos());
        verify(&db, work, expected)?;
        reopen_verify_ns.push(started.elapsed().as_nanos());
        close(db)?;
    }

    let backup = directory.join("backup.sqlite");
    let restored = directory.join("restored.sqlite");
    let backup_ns = verified_copy(&path, &backup, work, expected)?;
    let restore_ns = verified_copy(&backup, &restored, work, expected)?;
    let started = Instant::now();
    let db = connect(&path, false)?;
    db.execute_batch("CREATE TEMP TABLE keep(k BLOB,epoch INTEGER,PRIMARY KEY(k,epoch)) WITHOUT ROWID; BEGIN IMMEDIATE;")?;
    for epoch in expected.retained.keys() {
        db.execute("INSERT OR IGNORE INTO keep SELECT k,MAX(epoch) FROM versions WHERE epoch<=? GROUP BY k", [epoch])?;
    }
    db.execute_batch("DELETE FROM versions WHERE NOT EXISTS(SELECT 1 FROM keep WHERE keep.k=versions.k AND keep.epoch=versions.epoch); COMMIT;")?;
    let after_gc = version_rows(&db)?;
    let required: BTreeSet<_> = std::iter::once(&expected.current)
        .chain(expected.retained.values())
        .flat_map(|state| state.values())
        // Epoch and key together identify the exact version row.
        .map(|data| &data[..16])
        .collect();
    ensure!(
        after_gc == required.len() as u64,
        "GC version count mismatch"
    );
    db.execute_batch("VACUUM")?;
    checkpoint(&db)?;
    close(db)?;
    verify_path(&path, work, expected)?;
    sync_output(&path)?;
    let compact_ns = started.elapsed().as_nanos();
    Ok(json!({
        "commit_ns":commit_ns,"transaction_ns":transaction_ns,"pin_ns":pin_ns,
        "reopen_ns":reopen_ns,"reopen_verify_ns":reopen_verify_ns,
        "oracle_digest":expected.digest,"history_count":expected.retained.len(),
        "version_rows_before_gc":before_gc,"version_rows_after_gc":after_gc,
        "checkpoint_result":checkpoint_result,"checkpoint_ns":checkpoint_ns,
        "backup_ns":backup_ns,"restore_ns":restore_ns,"compact_ns":compact_ns,
        "sizes_live":sizes_live,"sizes_checkpointed":sizes_checkpointed,
        "sizes_closed":sizes_closed,"sizes_compacted":sizes(&path)?,
        "backup_sizes":sizes(&backup)?,"restored_sizes":sizes(&restored)?,
        "settings":settings,"compile_options":options,
    }))
}

fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 7,
        "SQLite comparator needs output and six workload parameters"
    );
    let parameter = |index: usize| -> Result<u64> {
        Ok(args[index]
            .to_str()
            .context("UTF-8 parameter required")?
            .parse()?)
    };
    let work = Work {
        commits: parameter(1)?,
        keys: parameter(2)?,
        batch: parameter(3)?,
        payload: parameter(4)?,
        retain_every: parameter(5)?,
        reopens: parameter(6)?,
    };
    work.validate()?;
    let identity = Connection::open_in_memory()?;
    let version = identity.query_row("SELECT sqlite_version(), sqlite_source_id()", [], |row| {
        Ok([row.get::<_, String>(0)?, row.get::<_, String>(1)?])
    })?;
    close(identity)?;
    let mut output = sqlite_run(&PathBuf::from(&args[0]), work, &work.expected())?;
    output["sqlite_version"] = json!(version[0]);
    output["sqlite_source_id"] = json!(version[1]);
    println!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::symlink,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "weekend-sqlite-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const WORK: Work = Work {
        commits: 8,
        keys: 5,
        batch: 3,
        payload: 32,
        retain_every: 3,
        reopens: 2,
    };

    #[test]
    fn existing_copy_targets_reject_before_opening_source() {
        let tmp = Scratch::new();
        let destination = tmp.0.join("sentinel");
        let dangling = tmp.0.join("link");
        let missing = tmp.0.join("missing");
        fs::write(&destination, b"preserve-existing-output").unwrap();
        symlink(&missing, &dangling).unwrap();
        let expected = WORK.expected();
        for target in [&destination, &dangling] {
            assert_eq!(
                verified_copy(&missing, target, WORK, &expected)
                    .unwrap_err()
                    .to_string(),
                "copy destination already exists"
            );
        }
        assert_eq!(fs::read(destination).unwrap(), b"preserve-existing-output");
        assert!(!missing.exists());
        assert!(fs::symlink_metadata(dangling).unwrap().is_symlink());
    }

    #[test]
    fn history_survives_reopen_gc_vacuum_backup_and_restore() {
        let tmp = Scratch::new();
        let expected = WORK.expected();
        assert_eq!(
            expected.retained.keys().copied().collect::<Vec<_>>(),
            [3, 6, 8]
        );
        let report = sqlite_run(&tmp.0.join("sqlite"), WORK, &expected).unwrap();
        assert_eq!(report["oracle_digest"], expected.digest);
        assert_eq!(report["version_rows_before_gc"], 29);
        assert!(report["version_rows_after_gc"].as_u64().unwrap() < 29);
        assert_eq!(report["sizes_closed"]["-wal"], 0);
        assert_eq!(report["sizes_checkpointed"]["-wal"], 0);
        assert_eq!(report["pin_ns"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn full_oracle_rejects_noncanonical_keys_and_corrupted_state() {
        let work = Work {
            commits: 1,
            keys: 1,
            batch: 1,
            payload: 16,
            retain_every: 1,
            reopens: 1,
        };
        let expected = work.expected();
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE versions(k BLOB,epoch INTEGER,payload BLOB,PRIMARY KEY(k,epoch));
            CREATE TABLE head(epoch INTEGER); CREATE TABLE retained(id INTEGER PRIMARY KEY,epoch INTEGER);
            INSERT INTO head VALUES(1); INSERT INTO retained VALUES(1,1);",
        ).unwrap();
        let data = &expected.current[&0];
        db.execute(
            "INSERT INTO versions VALUES(?,1,?)",
            params![[0_u8; 8], data],
        )
        .unwrap();
        verify(&db, work, &expected).unwrap();
        for key in [vec![0_u8; 1], vec![0_u8; 9]] {
            db.execute("INSERT INTO versions VALUES(?,1,?)", params![key, data])
                .unwrap();
            assert_eq!(
                verify(&db, work, &expected).unwrap_err().to_string(),
                "noncanonical binary key"
            );
            db.execute("DELETE FROM versions WHERE k=?", [key]).unwrap();
        }
        db.execute("UPDATE versions SET payload=?", [[0_u8; 16]])
            .unwrap();
        assert_eq!(
            verify(&db, work, &expected).unwrap_err().to_string(),
            "current state mismatch"
        );
        db.execute("UPDATE versions SET payload=?", [data]).unwrap();
        db.execute("UPDATE retained SET epoch=0", []).unwrap();
        assert_eq!(
            verify(&db, work, &expected).unwrap_err().to_string(),
            "retained catalog mismatch"
        );
    }

    #[test]
    fn retired_sqlite_controls_map_to_this_binary() {
        let ledger: Value =
            serde_json::from_str(include_str!("../../../xtask/tests/legacy-controls.json"))
                .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--list", "--format=terse"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        let names = text
            .lines()
            .filter_map(|line| line.strip_suffix(": test"))
            .collect::<BTreeSet<_>>();
        let rows = ledger["controls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["rust_binary"] == "sqlite-comparator")
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 3);
        for row in rows {
            for test in row["rust"].as_array().unwrap() {
                assert!(
                    names.contains(test.as_str().unwrap()),
                    "missing SQLite control: {test}"
                );
            }
        }
    }
}
