#![feature(coverage_attribute)]

#[inline(never)]
fn decision(a: bool, b: bool) -> bool {
    if a && b { true } else { false }
}

#[coverage(off)]
fn main() {
    use std::hint::black_box;
    assert!(decision(black_box(true), black_box(true)));
    if std::env::args().nth(1).as_deref() == Some("complete") {
        assert!(!decision(black_box(true), black_box(false)));
        assert!(!decision(black_box(false), black_box(true)));
    }
}
