//! Reusable checks over compatible, reachable states from one record history.
use std::fmt::Debug;
use weekend_db::crdt::Record;

fn merged<R: Record>(mut left: R, right: R) -> R {
    left.merge(right);
    left
}

fn assert_codec<R: Record + Debug>(state: R, bytes: &mut [u8], repeated: &mut [u8]) {
    bytes.fill(0);
    repeated.fill(0);
    state.encode(bytes).expect("reachable state must encode");
    state
        .encode(repeated)
        .expect("repeated encoding must succeed");
    assert_eq!(bytes, repeated, "encoding must be deterministic");
    assert_eq!(R::decode(bytes), Ok(state), "codec must preserve the state");
}

/// Verify merge laws for all pairs and triples of the supplied states.
/// Use states from one compatible history, including intermediate and stale
/// states. Test actor admission and conflict-resolution edits separately.
pub fn assert_merge_laws<R: Record + Debug>(states: &[R]) {
    assert!(!states.is_empty(), "provide at least one reachable state");
    let mut bytes = vec![0; R::SIZE];
    let mut repeated = vec![0; R::SIZE];
    for (i, &a) in states.iter().enumerate() {
        assert_codec(a, &mut bytes, &mut repeated);
        assert_eq!(merged(a, a), a, "merge is not idempotent at state {i}");
        for (j, &b) in states.iter().enumerate() {
            let ab = merged(a, b);
            assert_eq!(ab, merged(a, b), "merge is not deterministic at ({i}, {j})");
            assert_eq!(ab, merged(b, a), "merge is not commutative at ({i}, {j})");
            assert_codec(ab, &mut bytes, &mut repeated);
            for (k, &c) in states.iter().enumerate() {
                let left = merged(ab, c);
                assert_eq!(
                    left,
                    merged(a, merged(b, c)),
                    "merge is not associative at ({i}, {j}, {k})"
                );
                assert_codec(left, &mut bytes, &mut repeated);
            }
        }
    }
}
