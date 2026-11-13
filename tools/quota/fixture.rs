//! Real Linux quota fixture. The controller supplies the quota and private path;
//! this process neither fills a filesystem nor substitutes a storage backend.
#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("native_quota requires Linux and the private quota controller");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{fmt::Write, path::Path};
    use weekend_db::{
        CollectionId, Database, Error, Kind, Limits, Rows, Snapshot, Transaction, TransactionState,
        storage::{NativeIoCategory, NativeIoOperation},
        version,
    };

    const RETAINED_ID: u64 = 1;
    const RETAINED_REVISION: u64 = 2;

    fn baseline_bytes() -> Vec<u8> {
        (0..512).map(|i| ((i * 13 + 7) % 251) as u8).collect()
    }

    fn candidate_bytes() -> Vec<u8> {
        (0..65536).map(|i| ((i * 17 + 23) % 251) as u8).collect()
    }

    fn stage(tx: &mut Transaction<'_>, db: &Database, c: CollectionId, updated: bool) {
        let (integer, bytes) = if updated {
            (18, candidate_bytes())
        } else {
            (17, baseline_bytes())
        };
        tx.put(c, b"row", &db.integer(integer).unwrap()).unwrap();
        tx.put(c, b"payload", &db.bytes(&bytes).unwrap()).unwrap();
    }

    // Compare all actual scan rows and every payload byte before serializing the
    // same rows. Exact length and keys also rule out extra or missing records.
    fn checked_rows(view: &Snapshot<'_>, c: CollectionId, updated: bool) -> Rows {
        let rows = view.scan(c).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(view.count(c).unwrap(), 2);
        assert_eq!(rows[0].0, b"payload");
        assert_eq!(rows[0].1.kind(), Kind::Bytes);
        let expected = if updated {
            candidate_bytes()
        } else {
            baseline_bytes()
        };
        assert_eq!(rows[0].1.as_bytes().unwrap(), expected);
        assert_eq!(rows[1].0, b"row");
        assert_eq!(rows[1].1.kind(), Kind::Integer);
        assert_eq!(rows[1].1.as_i64().unwrap(), if updated { 18 } else { 17 });
        rows
    }

    struct State {
        current_revision: u64,
        retained_revision: u64,
        recovery_required: bool,
        current_rows: Rows,
        retained_rows: Rows,
    }

    fn checked_state(db: &Database, revision: u64, recovery_required: bool) -> State {
        assert!(matches!(revision, 3 | 4));
        let c = db.collection_id(1);
        let current = db.snapshot();
        assert_eq!(current.revision(), revision);
        assert_eq!(db.retained_ids().unwrap(), [RETAINED_ID]);
        let retained = db.retained_snapshot(RETAINED_ID).unwrap();
        assert_eq!(retained.revision(), RETAINED_REVISION);
        assert_eq!(db.requires_recovery(), recovery_required);
        State {
            current_revision: current.revision(),
            retained_revision: retained.revision(),
            recovery_required: db.requires_recovery(),
            current_rows: checked_rows(&current, c, revision == 4),
            retained_rows: checked_rows(&retained, c, false),
        }
    }

    #[derive(Default)]
    struct Failure {
        commit_error: i32,
        native_category: Option<NativeIoCategory>,
        native_raw_code: Option<i32>,
        native_operation: Option<NativeIoOperation>,
        terminal_error: i32,
        blocked_write_error: i32,
    }

    fn hex(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(DIGITS[(byte >> 4) as usize] as char);
            out.push(DIGITS[(byte & 15) as usize] as char);
        }
        out
    }

    fn rows_json(rows: &Rows) -> String {
        let mut out = String::from("[");
        for (i, (key, value)) in rows.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            match value.kind() {
                Kind::Bytes => write!(
                    out,
                    "{{\"key_hex\":\"{}\",\"kind\":\"bytes\",\"bytes_hex\":\"{}\"}}",
                    hex(key),
                    hex(value.as_bytes().unwrap())
                )
                .unwrap(),
                Kind::Integer => write!(
                    out,
                    "{{\"key_hex\":\"{}\",\"kind\":\"integer\",\"integer\":{}}}",
                    hex(key),
                    value.as_i64().unwrap()
                )
                .unwrap(),
                kind => panic!("unexpected row kind {kind:?}"),
            }
        }
        out.push(']');
        out
    }

    fn emit(mode: &str, state: State, failure: Failure) {
        let category = failure
            .native_category
            .map_or_else(|| "None".to_owned(), |v| format!("{v:?}"));
        let operation = failure
            .native_operation
            .map_or_else(|| "None".to_owned(), |v| format!("{v:?}"));
        let raw_code = failure
            .native_raw_code
            .map_or_else(|| "null".to_owned(), |v| v.to_string());
        // Source is checked to be a full lowercase Git SHA below; all other
        // string fields are fixed enums, fixed modes, or hexadecimal bytes.
        println!(
            "{{\"stage\":\"{mode}\",\"source\":\"{}\",\"features\":{},\"pid\":{},\"current_revision\":{},\"retained_id\":{RETAINED_ID},\"retained_revision\":{},\"commit_error\":{},\"native_category\":\"{category}\",\"native_raw_code\":{raw_code},\"native_operation\":\"{operation}\",\"terminal_error\":{},\"recovery_required\":{},\"blocked_write_error\":{},\"current_rows\":{},\"retained_rows\":{}}}",
            version::SOURCE_ID,
            version::FEATURES,
            std::process::id(),
            state.current_revision,
            state.retained_revision,
            failure.commit_error,
            failure.terminal_error,
            state.recovery_required,
            failure.blocked_write_error,
            rows_json(&state.current_rows),
            rows_json(&state.retained_rows),
        );
    }

    pub fn run() {
        assert_eq!(version::FEATURES, 0);
        assert_eq!(
            version::SOURCE_ID,
            std::env::var("WEEKEND_DB_SOURCE_ID").expect("controller supplies exact source SHA")
        );
        assert_eq!(version::SOURCE_ID.len(), 40);
        assert!(
            version::SOURCE_ID
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        let args: Vec<_> = std::env::args_os().collect();
        let mode = args.get(1).and_then(|s| s.to_str()).expect("command");
        assert!(matches!(mode, "seed" | "fail" | "inspect" | "retry"));
        assert_eq!(args.len(), if mode == "inspect" { 4 } else { 3 });
        let path = Path::new(&args[2]);
        let db = if mode == "seed" {
            Database::create_file(path, Limits::default()).unwrap()
        } else {
            Database::open_file(path, Limits::default()).unwrap()
        };
        let mut failure = Failure::default();
        let revision = match mode {
            "seed" => {
                assert_eq!(db.snapshot().revision(), 0);
                let c = db.create_collection().unwrap();
                assert_eq!(c.as_u64(), 1);
                assert_eq!(db.snapshot().revision(), 1);
                let mut tx = db.begin();
                stage(&mut tx, &db, c, false);
                assert_eq!(tx.commit(), Ok(2));
                let baseline = db.snapshot();
                assert_eq!(baseline.revision(), RETAINED_REVISION);
                assert_eq!(db.retain_snapshot(&baseline), Ok(RETAINED_ID));
                3
            }
            "fail" => {
                checked_state(&db, 3, false);
                let mut tx = db.begin();
                stage(&mut tx, &db, db.collection_id(1), true);
                let failed = tx.commit_with_diagnostics().unwrap_err();
                assert_eq!(failed.error, Error::Io);
                assert_eq!(failed.verification, None);
                let native = failed.native.expect("real quota error has native context");
                assert_eq!(native.category, NativeIoCategory::Quota);
                assert_eq!(native.raw_os_error, Some(122));
                assert_eq!(native.operation, NativeIoOperation::Write);
                assert_eq!(tx.state(), TransactionState::Aborted);
                let terminal = tx.commit().unwrap_err();
                assert_eq!(terminal, Error::Closed);
                assert!(db.requires_recovery());
                let mut blocked = db.begin();
                stage(&mut blocked, &db, db.collection_id(1), true);
                let blocked_error = blocked.commit().unwrap_err();
                assert_eq!(blocked_error, Error::RecoveryRequired);
                assert_eq!(blocked.state(), TransactionState::Failed);
                failure = Failure {
                    commit_error: failed.error as i32,
                    native_category: Some(native.category),
                    native_raw_code: native.raw_os_error,
                    native_operation: Some(native.operation),
                    terminal_error: terminal as i32,
                    blocked_write_error: blocked_error as i32,
                };
                3
            }
            "inspect" => {
                let revision = args[3].to_str().unwrap().parse::<u64>().unwrap();
                assert!(matches!(revision, 3 | 4));
                revision
            }
            "retry" => {
                checked_state(&db, 3, false);
                let mut tx = db.begin();
                stage(&mut tx, &db, db.collection_id(1), true);
                assert_eq!(tx.commit(), Ok(4));
                4
            }
            _ => unreachable!(),
        };
        let state = checked_state(&db, revision, mode == "fail");
        emit(mode, state, failure);
    }
}
