use weekend_db::{Database, Error, Limits};

fn limits(max_live_bytes: usize) -> Limits {
    Limits {
        max_live_bytes,
        ..Limits::default()
    }
}

#[test]
fn initial_head_requires_exact_live_layout_budget() {
    // Discover this platform's requested layout without fixing ABI padding.
    let reference = Database::new().unwrap();
    let head_bytes = reference.memory_usage().live_bytes;
    assert!(head_bytes > 0);
    assert_eq!(reference.memory_usage().limit_bytes, usize::MAX);
    for cap in [0, head_bytes - 1] {
        assert!(matches!(
            Database::with_limits(limits(cap)),
            Err(Error::Limit)
        ));
    }
    let exact = Database::with_limits(limits(head_bytes)).unwrap();
    assert_eq!(exact.memory_usage().live_bytes, head_bytes);
    assert_eq!(exact.memory_usage().peak_bytes, head_bytes);
    assert!(matches!(exact.integer(1), Err(Error::Limit)));
    assert_eq!(exact.memory_usage().live_bytes, head_bytes);
    assert_eq!(exact.snapshot().revision(), 0);
}

#[test]
fn value_node_admission_failure_releases_already_allocated_payload() {
    let probe = Database::new().unwrap();
    let base = probe.memory_usage().live_bytes;
    let value = probe.bytes(&[7; 8]).unwrap();
    let exact_cap = probe.memory_usage().live_bytes;
    assert!(exact_cap > base + 8); // payload and its shared node both count
    drop(value);
    assert_eq!(probe.memory_usage().live_bytes, base);

    let short = Database::with_limits(limits(exact_cap - 1)).unwrap();
    assert!(matches!(short.bytes(&[7; 8]), Err(Error::Limit)));
    assert_eq!(short.memory_usage().live_bytes, base);
    // The payload allocation happened before the failed node admission.
    assert_eq!(short.memory_usage().peak_bytes, base + 8);
    assert_eq!(short.snapshot().revision(), 0);
    assert!(!short.requires_recovery());

    let exact = Database::with_limits(limits(exact_cap)).unwrap();
    let value = exact.bytes(&[7; 8]).unwrap();
    assert_eq!(value.as_bytes().unwrap(), &[7; 8]);
    assert_eq!(exact.memory_usage().live_bytes, exact_cap);
    assert_eq!(exact.memory_usage().peak_bytes, exact_cap);
    let alias = value.clone();
    drop(value);
    assert_eq!(exact.memory_usage().live_bytes, exact_cap);
    drop(alias);
    assert_eq!(exact.memory_usage().live_bytes, base);
    assert_eq!(exact.memory_usage().peak_bytes, exact_cap);
    assert!(exact.bytes(&[7; 8]).is_ok());
}
