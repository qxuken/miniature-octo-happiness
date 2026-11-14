//! Small exact-rlib consumer for the measured intermediate copy budget.
//! Compile with the matching pinned rustc and --extern weekend_db=...rlib.
use weekend_db::{Error, Limits, storage, version};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 5);
    assert_eq!(version::SOURCE_ID, args[4]);
    let expected = match args[3].as_str() {
        "limit" => Err(Error::Limit),
        "ok" => Ok(()),
        _ => panic!("expected limit or ok"),
    };
    let result = storage::backup_file(
        &args[1],
        &args[2],
        Limits {
            max_live_bytes: 160 * 1024 * 1024,
            ..Limits::default()
        },
    );
    assert_eq!(result.map(|_| ()), expected);
    println!(
        "{{\"source_id\":\"{}\",\"copy_budget\":167772160,\"outcome\":\"{}\"}}",
        version::SOURCE_ID,
        args[3]
    );
}
