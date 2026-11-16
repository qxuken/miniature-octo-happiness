//! Independent six-case C ABI publication/recovery oracle. No engine imports.
use crate::delivery;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
pub fn package(root: &Path, library: &str) -> Result<Value> {
    let info = delivery::verified(root)?;
    ensure!(
        root.join(library).is_file() && info["features"] == "crdt,replication",
        "requires combined native package with {library}"
    );
    Ok(info)
}
fn validate(row: &Value, mode: u32) -> Result<Value> {
    ensure!(mode < 6, "unknown delivered fault mode");
    let expected = json!({"mode":mode,"status":if mode==0 {0}else if [1,5].contains(&mode){14}else{15},"visible":if mode==0 {4}else{3},"reopened":if [0,2,3].contains(&mode){4}else{3},"final":4,"retained_revision":2,"hits":u32::from([1,2,3,4].contains(&mode)),"syncs":([3,1,2,3,1,0][mode as usize]),"root_writes":u32::from([0,2,3,4].contains(&mode)),"witness_writes":u32::from([0,3].contains(&mode)),"native_errno":if mode==5 {28}else{0},"verified":true});
    ensure!(*row == expected, "unexpected ABI/state/fault outcome");
    Ok(row.clone())
}

pub fn full_space_phase(row: &Value, full: bool) -> Result<()> {
    let key = if full { "reopened" } else { "seeded" };
    ensure!(
        *row == json!({"mode":5,(key):3,"retained_revision":2,"verified":true}),
        "invalid seed or full-space reopen observation"
    );
    Ok(())
}
pub fn combine(exercise: &Value, recovery: &Value, mode: u32) -> Result<Value> {
    fn keys(row: &Value) -> Option<BTreeSet<&str>> {
        row.as_object()
            .map(|m| m.keys().map(String::as_str).collect::<BTreeSet<_>>())
    }
    ensure!(
        keys(exercise)
            == Some(
                [
                    "mode",
                    "status",
                    "visible",
                    "hits",
                    "syncs",
                    "root_writes",
                    "witness_writes",
                    "native_errno",
                    "verified"
                ]
                .into_iter()
                .collect()
            )
            && keys(recovery)
                == Some(
                    ["mode", "reopened", "final", "retained_revision", "verified"]
                        .into_iter()
                        .collect()
                ),
        "missing independent exercise/recovery phase"
    );
    ensure!(
        exercise["mode"].as_u64() == Some(mode as u64)
            && recovery["mode"].as_u64() == Some(mode as u64)
            && exercise["verified"] == true
            && recovery["verified"] == true,
        "phase identity or completion mismatch"
    );
    let mut row = exercise.as_object().unwrap().clone();
    row.extend(recovery.as_object().unwrap().clone());
    validate(&Value::Object(row), mode)
}
pub fn self_check() -> Result<()> {
    // A hand-fixed table, independent of validate's mode predicates.
    for (
        mode,
        [
            status,
            visible,
            reopened,
            hits,
            syncs,
            roots,
            witnesses,
            native,
        ],
    ) in [
        [0, 4, 4, 0, 3, 1, 1, 0],
        [14, 3, 3, 1, 1, 0, 0, 0],
        [15, 3, 4, 1, 2, 1, 0, 0],
        [15, 3, 4, 1, 3, 1, 1, 0],
        [15, 3, 3, 1, 1, 1, 0, 0],
        [14, 3, 3, 0, 0, 0, 0, 28],
    ]
    .into_iter()
    .enumerate()
    {
        let row = json!({"mode":mode,"status":status,"visible":visible,"reopened":reopened,"final":4,"retained_revision":2,"hits":hits,"syncs":syncs,"root_writes":roots,"witness_writes":witnesses,"native_errno":native,"verified":true});
        validate(&row, mode as u32)?;
        for field in [
            "status",
            "visible",
            "reopened",
            "hits",
            "native_errno",
            "final",
            "retained_revision",
            "syncs",
            "root_writes",
            "witness_writes",
            "mode",
        ] {
            let mut bad = row.clone();
            bad[field] = json!(99);
            ensure!(
                validate(&bad, mode as u32).is_err(),
                "oracle accepted corrupt {field}"
            );
        }
        let mut bad = row;
        bad["verified"] = json!(1);
        ensure!(
            validate(&bad, mode as u32).is_err(),
            "oracle accepted numeric verified"
        );
    }
    phase_controls()
}
fn phase_controls() -> Result<()> {
    let exercise = json!({"mode":1,"status":14,"visible":3,"hits":1,"syncs":1,"root_writes":0,"witness_writes":0,"native_errno":0,"verified":true});
    let recovery = json!({"mode":1,"reopened":3,"final":4,"retained_revision":2,"verified":true});
    combine(&exercise, &recovery, 1)?;
    let mut merged = exercise.as_object().unwrap().clone();
    merged.extend(recovery.as_object().unwrap().clone());
    ensure!(
        combine(&Value::Object(merged), &recovery, 1).is_err(),
        "exercise cannot assert recovery"
    );
    ensure!(
        combine(&exercise, &json!({}), 1).is_err(),
        "missing recovery"
    );
    for (target, field, value) in [
        (0, "mode", json!(true)),
        (1, "mode", json!(1.0)),
        (1, "mode", json!(2)),
        (1, "verified", json!(false)),
    ] {
        let mut a = exercise.clone();
        let mut b = recovery.clone();
        if target == 0 {
            a[field] = value;
        } else {
            b[field] = value;
        }
        ensure!(
            combine(&a, &b, 1).is_err(),
            "oracle accepted malformed phase"
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outcomes_reject_false_acknowledgment_and_partial_commit() {
        self_check().unwrap();
    }
    #[test]
    fn exercise_cannot_assert_independent_recovery() {
        phase_controls().unwrap();
    }
    #[test]
    fn apfs_seed_and_full_space_reopen_are_distinct_from_retry() {
        let mut full = json!({"mode":5,"reopened":3,"retained_revision":2,"verified":true});
        assert!(full_space_phase(&full, true).is_ok());
        assert!(full_space_phase(&full, false).is_err());
        full["final"] = json!(4);
        assert!(full_space_phase(&full, true).is_err());
        assert!(
            full_space_phase(
                &json!({"mode":5,"seeded":3,"retained_revision":2,"verified":true}),
                false
            )
            .is_ok()
        );
    }
}
