#![feature(coverage_attribute)]

#[inline(never)]
fn decision(a: bool, b: bool, c: bool) -> bool {
    if (a && b) || c { true } else { false }
}

#[coverage(off)]
fn main() {
    use std::hint::black_box;
    let mode = std::env::args().nth(1).unwrap();
    let cases: &[(bool, bool, bool, bool)] = match mode.as_str() {
        "partial" => &[(true, true, false, true)],
        // Every evaluated condition has both outcomes. B still has no pair
        // changing the decision: its false case is masked by C=true.
        "branch-complete" => &[
            (true, true, false, true),
            (true, false, true, true),
            (false, true, false, false),
            (false, true, true, true),
        ],
        // Adds the missing independent effect of B with A=true/C=false.
        "complete" => &[
            (true, true, false, true),
            (true, false, true, true),
            (false, true, false, false),
            (false, true, true, true),
            (true, false, false, false),
        ],
        _ => panic!("unknown calibration case"),
    };
    for &(a, b, c, expected) in cases {
        assert_eq!(decision(black_box(a), black_box(b), black_box(c)), expected);
    }
}
