#[path = "../../apps/study-peer/src/native/protocol.rs"]
mod protocol;
#[allow(dead_code)]
#[path = "../../examples/support/study.rs"]
mod study;
use std::ffi::OsStr;
use weekend_db::{Database, crdt::Replica};

pub fn peer_transport(input: &[u8]) {
    let input = &input[..input.len().min(2048)];
    if let Ok(text) = std::str::from_utf8(input) {
        let actor = ["a", "b", "invalid"][usize::from(input.first().copied().unwrap_or(0) % 3)];
        let expected = actor != "invalid"
            && text.len() == 32
            && text.bytes().all(|b| b.is_ascii_hexdigit())
            && text.bytes().any(|b| b != b'0');
        assert_eq!(
            protocol::Enrollment::parse(OsStr::new(actor), OsStr::new(text)).is_ok(),
            expected
        );
    }
    #[cfg(unix)]
    if std::str::from_utf8(input).is_err() {
        use std::os::unix::ffi::OsStrExt;
        assert!(protocol::Enrollment::parse(OsStr::new("a"), OsStr::from_bytes(input)).is_err());
    }
    let enrollment = protocol::Enrollment::parse(
        OsStr::new("a"),
        OsStr::new("11111111111111111111111111111111"),
    )
    .unwrap();
    let db = Database::new().unwrap();
    let c = db.create_collection().unwrap();
    let replica =
        Replica::<study::StudyProgress>::enroll(&db, c, enrollment.policy, enrollment.actor)
            .unwrap();
    let (mut total, mut mask, mut goal, mut limit) = (0u64, 0u64, None, None);
    for op in input.chunks(5).take(32) {
        let kind = op[0] % 5;
        let lesson = op.get(1).copied().unwrap_or(0);
        let minutes = u64::from(op.get(2).copied().unwrap_or(0));
        let g = u64::from(op.get(3).copied().unwrap_or(0));
        let l = u64::from(op.get(4).copied().unwrap_or(0));
        let command = match kind {
            0 => "show".into(),
            1 => format!("practice {minutes}"),
            2 => format!("edit {lesson} {minutes} {g} {l}"),
            3 => "practice 18446744073709551616".into(),
            _ => "edit 1 2 3".into(),
        };
        let before = db.snapshot().revision();
        let result = protocol::command(
            &command.split_whitespace().collect::<Vec<_>>(),
            &db,
            &replica,
        );
        let valid = kind == 0 || (kind == 1 || kind == 2 && lesson < 64) && minutes > 0;
        assert_eq!(result.is_ok(), valid, "{command}: {result:?}");
        if !valid || kind == 0 {
            assert_eq!(db.snapshot().revision(), before);
        }
        if valid && kind > 0 {
            total += minutes;
            if kind == 2 {
                mask |= 1u64 << lesson;
                goal = Some(g);
                limit = Some(l);
            }
        }
        let state = replica.read().unwrap();
        assert_eq!(state.practice_minutes.total(), total);
        assert_eq!(
            state.completed.iter().collect::<Vec<_>>(),
            (0..64u8)
                .filter(|id| mask & (1u64 << id) != 0)
                .collect::<Vec<_>>()
        );
        assert_eq!(state.effective_weekly_goal(), goal);
        assert_eq!(state.session_limit.values().next(), limit);
    }
    // Arbitrary local command text exercises parser diagnostics. Failure cannot
    // publish partial mutations. Successful semantics are modeled above.
    if let Ok(raw) = std::str::from_utf8(input) {
        let before = (db.snapshot().revision(), replica.read().unwrap());
        if protocol::command(&raw.split_whitespace().collect::<Vec<_>>(), &db, &replica).is_err() {
            assert_eq!(before, (db.snapshot().revision(), replica.read().unwrap()));
        }
    }
}
#[test]
fn replay_peer() {
    for input in [
        b"practice 18446744073709551616".as_slice(),
        b"edit 64 1 1 1",
        b"11111111111111111111111111111111",
        &[2, 63, 10, 90, 20, 2, 64, 10, 90, 20, 1, 0, 0, 0, 0],
    ] {
        peer_transport(input);
    }
}
