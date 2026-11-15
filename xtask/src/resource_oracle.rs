//! Independent application-level byte and state oracle for the compound fixture.
//! There are no product dependencies; replay the deterministic payload directly.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::sync::OnceLock;
pub const MIB: u64 = 1024 * 1024;
pub const CORE: u64 = 96 * MIB;
const NATIVE: [&str; 11] = [
    "fixture",
    "drafts_ready",
    "core_limit_rejected",
    "drafts_released",
    "contended_commits",
    "image_limit_rejected",
    "reopen",
    "backup_budget_rejected",
    "backup",
    "restore",
    "compact",
];
const FAULTS: [&str; 8] = [
    "allocation",
    "append_drop",
    "append_persist",
    "root_drop",
    "root_persist",
    "witness_drop",
    "witness_persist",
    "acknowledged",
];
pub fn number(row: &Value, key: &str) -> Result<u64> {
    row[key]
        .as_u64()
        .with_context(|| format!("non-integer or missing {key}"))
}
pub fn integers(value: &Value, n: usize) -> bool {
    value
        .as_array()
        .is_some_and(|r| r.len() == n && r.iter().all(|x| x.as_u64().is_some()))
}
pub fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(14_695_981_039_346_656_037, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1_099_511_628_211)
    })
}
pub fn payload(epoch: u64, length: u64) -> impl Iterator<Item = u8> {
    let prefix = epoch.to_le_bytes();
    (0..length).map(move |i| {
        if i < 8 {
            prefix[i as usize]
        } else {
            ((epoch * 13 + i) % 251) as u8
        }
    })
}
fn fixed_digests() -> [u64; 3] {
    static BASE: OnceLock<[u64; 3]> = OnceLock::new();
    *BASE.get_or_init(|| {
        [
            fnv((100..148).flat_map(|epoch| payload(epoch, 896 * 1024))),
            fnv([64]
                .into_iter()
                .chain(1..=64)
                .flat_map(|epoch| payload(epoch, 8192))),
            fnv([0; 64 * 8 * 8]),
        ]
    })
}
pub fn digests(seq: u64, changed: bool) -> [u64; 4] {
    // The oracle only accepts reviewed fixture states, not attacker-selected
    // epoch arithmetic or unbounded dimensions.
    assert!(matches!(seq, 0 | 4));
    static STATES: [OnceLock<[u64; 4]>; 4] = [const { OnceLock::new() }; 4];
    let index = usize::from(seq == 4) * 2 + usize::from(changed);
    *STATES[index].get_or_init(|| {
        let [bulk, deep, retained] = fixed_digests();
        let writers = fnv((0..8).flat_map(|w| {
            let actual = seq + u64::from(changed && w == 0);
            actual
                .to_le_bytes()
                .into_iter()
                .chain(payload(1000 + w * 10 + actual, 65536))
        }));
        [bulk, deep, writers, retained]
    })
}
pub fn completion(profile: &str, source: &str) -> Value {
    json!({"complete":true,"profile":profile,"source_id":source,"features":4,"abi":1,"format":2})
}
fn state_at(
    row: &Value,
    phase: &str,
    revision: u64,
    seq: u64,
    changed: bool,
    min_file: u64,
) -> Result<()> {
    ensure!(
        row["phase"] == phase && row["revision"] == revision,
        "wrong phase/revision: {phase}"
    );
    ensure!(
        row["seq"] == seq && row["changed"] == changed && row["roots"] == 64,
        "wrong root/current state: {phase}"
    );
    ensure!(
        row["digests"] == json!(digests(seq, changed)),
        "independent root byte oracle: {phase}"
    );
    let live = number(row, "live_bytes")?;
    let peak = number(row, "peak_bytes")?;
    let cap = number(row, "cap_bytes")?;
    let bytes = number(row, "file_bytes")?;
    number(row, "elapsed_ns")?;
    ensure!(
        live > 0 && live <= peak && peak <= cap && cap == CORE && number(row, "rss_kib")? > 0,
        "invalid core/RSS: {phase}"
    );
    ensure!(
        bytes > min_file && bytes <= 64 * MIB,
        "not a reachable near-cap image: {phase}"
    );
    Ok(())
}
pub fn state(
    row: &Value,
    phase: &str,
    revision: u64,
    changed: bool,
    file_bytes: Option<u64>,
) -> Result<()> {
    state_at(row, phase, revision, 4, changed, 60 * MIB)?;
    ensure!(
        file_bytes.is_none_or(|n| row["file_bytes"] == n),
        "wrong replacement image length"
    );
    Ok(())
}
pub fn compound(rows: &[Value], profile: &str, source: &str) -> Result<()> {
    let phases: &[&str] = match profile {
        "native" => &NATIVE,
        "faults" => &FAULTS,
        "inspect" => &["fresh_process_reopen"],
        _ => anyhow::bail!("unknown compound profile"),
    };
    ensure!(
        rows.len() == phases.len() + 1 && rows.last() == Some(&completion(profile, source)),
        "wrong compound build identity or phase count"
    );
    for (i, (row, phase)) in rows.iter().zip(phases).enumerate() {
        let changed = profile == "faults";
        let seq = if profile == "native" && i < 4 { 0 } else { 4 };
        let survives = changed && i >= 4;
        let revision = if changed {
            213 + u64::from(survives)
        } else if seq == 0 {
            180
        } else {
            212
        };
        state_at(row, phase, revision, seq, changed, 64 * MIB * 9 / 10)?;
        let detail = &row["detail"];
        match *phase {
            "drafts_ready" => ensure!(
                detail["drafts"] == 8 && integers(&detail["prepare_ns"], 8),
                "missing overlapping drafts"
            ),
            "core_limit_rejected" => ensure!(
                number(row, "live_bytes")? > CORE * 99 / 100
                    && *detail == json!({"error":"Limit","drafts":8}),
                "missing saturated rejection"
            ),
            "drafts_released" => ensure!(
                detail["baseline_live"] == row["live_bytes"],
                "draft charge not reclaimed"
            ),
            "contended_commits" => ensure!(
                detail["writers"] == 8
                    && detail["rounds"] == 4
                    && integers(&detail["commit_ns"], 32)
                    && integers(&detail["reader_ns"], 256),
                "incomplete concurrent samples"
            ),
            "image_limit_rejected" => ensure!(
                *detail == json!({"error":"Limit","new_payload_bytes":4*MIB}),
                "missing image-limit rejection"
            ),
            "backup_budget_rejected" => ensure!(
                *detail == json!({"copy_budget":CORE,"error":"Limit"}),
                "wrong copy domain boundary"
            ),
            "backup" | "restore" | "compact" => ensure!(
                *detail == json!({"copy_budget":256*MIB}),
                "wrong copy budget"
            ),
            _ => (),
        }
        if changed {
            let error = match *phase {
                "allocation" => "OutOfMemory",
                "append_drop" | "append_persist" => "Io",
                "acknowledged" => "Ok",
                _ => "OutcomeUnknown",
            };
            let pressure = number(detail, "pressure_live_bytes")?;
            ensure!(
                detail["error"] == error
                    && detail["reopened_revision"] == 212 + u64::from(survives)
                    && pressure > CORE * 89 / 100
                    && pressure < CORE
                    && detail["pending_drafts"] == 2,
                "wrong durability outcome or pressure: {phase}"
            );
        }
    }
    if profile == "native" {
        for group in [&rows[..4], &rows[4..8]] {
            ensure!(
                group
                    .iter()
                    .all(|r| r["file_bytes"] == group[0]["file_bytes"]),
                "rejection/pressure/reopen changed image length"
            );
        }
        ensure!(
            rows[0]["live_bytes"] == rows[3]["live_bytes"],
            "baseline charge not restored"
        );
    }
    Ok(())
}

/// The finite single-lane resource driver emits telemetry plus an independent
/// digest of the application payloads. Check its order and each root's contents.
pub fn envelope(rows: &[Value]) -> Result<()> {
    const PHASES: [&str; 20] = [
        "nested_first_root",
        "nested_64_roots",
        "nested_reopen",
        "nested_reopen",
        "nested_reopen",
        "draft_pressure",
        "core_limit_rejected",
        "drafts_released_admission_recovers",
        "pins_released_admission_recovers",
        "pins_released_reopen_reclaims_cache",
        "synthetic_exact_image_boundary",
        "synthetic_compact_reopens_admission",
        "reachable_natural_image_limit",
        "reachable_reopen",
        "reachable_reopen",
        "reachable_reopen",
        "reachable_backup",
        "reachable_restore",
        "reachable_compact",
        "bounded_stack_depth64",
    ];
    ensure!(
        rows.len() == PHASES.len() + 1 && rows.last() == Some(&json!({"complete":true,"cases":4})),
        "incomplete resource phase set"
    );
    let natural = &rows[12];
    let count = number(&natural["detail"], "accepted_keys")?;
    ensure!(
        (8..80).contains(&count),
        "missing natural image-limit rejection"
    );
    ensure!(
        integers(&natural["detail"]["commit_ns"], count as usize),
        "commit sample count mismatch"
    );
    let mut digests = std::collections::BTreeMap::new();
    for (row, phase) in rows.iter().zip(PHASES) {
        ensure!(
            row["phase"] == phase,
            "resource phase ordering mismatch: {phase}"
        );
        let (live, peak, cap) = (
            number(row, "live_bytes")?,
            number(row, "peak_bytes")?,
            number(row, "cap_bytes")?,
        );
        ensure!(
            live <= peak && peak <= cap,
            "core accounting invalid: {phase}"
        );
        ensure!(
            number(row, "rss_kib")? > 0 && number(row, "file_bytes")? <= 64 * MIB,
            "resource sample invalid: {phase}"
        );
        let roots = number(row, "retained_count")?;
        let expected = match row["oracle_kind"].as_str() {
            Some("large") => {
                ensure!(
                    row["current_count"] == count && roots == 8,
                    "large root set mismatch"
                );
                *digests.entry(("large", count)).or_insert_with(|| {
                    fnv((1..=count)
                        .chain((1..=8).flat_map(|root| 1..=root))
                        .flat_map(|epoch| payload(epoch, 896 * 1024)))
                })
            }
            Some("nested" | "stack") => {
                ensure!(row["current_count"] == 1, "nested row count mismatch");
                if phase == "nested_first_root" || row["oracle_kind"] == "stack" {
                    ensure!(roots == 1, "one-root fixture mismatch");
                    *digests.entry(("nested", 1)).or_insert_with(|| {
                        fnv([1, 1].into_iter().flat_map(|epoch| payload(epoch, 8192)))
                    })
                } else {
                    ensure!(matches!(roots, 0 | 64), "nested root count mismatch");
                    *digests.entry(("nested", roots)).or_insert_with(|| {
                        fnv([64]
                            .into_iter()
                            .chain(1..=roots)
                            .flat_map(|epoch| payload(epoch, 8192)))
                    })
                }
            }
            _ => anyhow::bail!("unknown resource oracle kind: {phase}"),
        };
        ensure!(
            row["oracle_digest"] == expected,
            "independent byte oracle mismatch: {phase}"
        );
    }
    for field in ["commit_ns", "pin_ns"] {
        ensure!(
            integers(&rows[1]["detail"][field], 64),
            "nested sample count mismatch"
        );
    }
    let pressure = &rows[6];
    ensure!(
        u128::from(number(pressure, "live_bytes")?) * 100
            > u128::from(number(pressure, "cap_bytes")?) * 95,
        "not near core cap"
    );
    ensure!(
        number(natural, "file_bytes")? > 64 * MIB * 9 / 10,
        "reachable image below 90 percent"
    );
    Ok(())
}

/// Independent schedule/outcome oracle for the injected second failures and
/// the killed native maintenance worker on both sides of selected boundaries.
pub fn resilience(rows: &[Value], profile: &str, source: &str) -> Result<()> {
    ensure!(
        rows.len() > 1 && rows.last() == Some(&completion(profile, source)),
        "wrong resilience completion identity"
    );
    let count;
    if matches!(profile, "maintenance-copy" | "maintenance-publish") {
        let publish = profile == "maintenance-publish";
        count = if publish { 16 } else { 36 };
        ensure!(
            rows.len() == 1 + count * 3 + 2,
            "incomplete maintenance cases"
        );
        let fresh = json!({"fresh_process":true});
        state(&rows[0], "control", 212, false, Some(63_074_304))?;
        ensure!(rows[0]["detail"] == fresh, "missing fresh control");
        #[derive(Clone, Copy)]
        enum Occurrence {
            First,
            Middle,
            Last,
        }
        use Occurrence::*;
        let mut schedule = Vec::new();
        if publish {
            schedule.extend((9..17).map(|step| (step, First)));
        } else {
            schedule.extend((0..9).map(|step| (step, First)));
            schedule.extend([(3, Last), (7, Middle), (7, Last)]);
            schedule.extend(
                [1, 5, 6]
                    .into_iter()
                    .flat_map(|step| [(step, Middle), (step, Last)]),
            );
        }
        let mut counts = std::collections::BTreeMap::new();
        let beginnings = schedule
            .into_iter()
            .flat_map(|(step, select)| [false, true].map(|after| (step, select, after)));
        for ((step, selection, after), [boundary, killed, retried]) in
            beginnings.zip(rows[1..].as_chunks::<3>().0)
        {
            let observed = number(boundary, "count")?;
            ensure!(
                boundary["step"] == step && observed > 0,
                "wrong boundary/count"
            );
            if matches!(step, 3 | 7) {
                ensure!(
                    observed == if step == 3 { 2 } else { 3 },
                    "missing durability barrier"
                );
            }
            ensure!(
                *counts.entry(step).or_insert(observed) == observed,
                "inconsistent calibration count"
            );
            let chosen = match selection {
                First => 0,
                Middle => observed / 2,
                Last => observed - 1,
            };
            ensure!(
                boundary["occurrence"] == chosen
                    && (matches!(selection, First)
                        || observed > if matches!(step, 3 | 7) { 1 } else { 2 }),
                "wrong sampled large-I/O occurrence"
            );
            let name = format!(
                "s{step}_n{chosen}_{}",
                if after { "after" } else { "before" }
            );
            let exchanged = step > 11 || (step == 11 && after);
            ensure!(
                boundary["after"] == after
                    && boundary["boundary"] == name
                    && boundary["signal"] == 9
                    && boundary["exchanged"] == exchanged,
                "missing kill/exchange evidence"
            );
            ensure!(
                number(boundary, "orphans")? <= 1,
                "unexpected private namespace"
            );
            state(
                killed,
                &format!("killed_{name}"),
                212,
                false,
                Some(if exchanged { 63_074_304 } else { 66_154_496 }),
            )?;
            state(
                retried,
                &format!("retried_{name}"),
                212,
                false,
                Some(63_074_304),
            )?;
            ensure!(
                killed["detail"] == fresh && retried["detail"] == fresh,
                "not checked by fresh processes"
            );
        }
    } else {
        let initial = profile
            .strip_prefix("recovery-")
            .context("unknown resilience profile")?
            .parse::<u64>()?;
        ensure!(
            initial <= 5 && profile == format!("recovery-{initial}"),
            "unknown recovery profile"
        );
        count = 18;
        ensure!(rows.len() == count * 2 + 2, "incomplete recovery cases");
        let schedule = (0..9u64).flat_map(|second| [false, true].map(|persist| (second, persist)));
        for ((second, persist), [recovered, retried]) in schedule.zip(rows.as_chunks::<2>().0) {
            let survives = initial >= 3 || (initial == 2 && (persist || (3..=7).contains(&second)));
            let revision = 212 + u64::from(survives);
            let name = format!(
                "i{initial}_f{second}_{}",
                if persist { "persist" } else { "drop" }
            );
            state(
                recovered,
                &format!("recovered_{name}"),
                revision,
                survives,
                None,
            )?;
            state(
                retried,
                &format!("retried_{name}"),
                revision + 1,
                true,
                None,
            )?;
            let floor = if initial == 5 { 213 } else { 212 };
            let detail = json!({"initial":initial,"second":second,"persist":persist,"recovery_error":if second==8{"Limit"}else{"Io"},"repeat_error":"Io","acknowledged_floor":floor});
            ensure!(
                recovered["detail"] == detail && revision >= floor,
                "wrong failure/floor"
            );
            ensure!(
                retried["detail"] == json!({"fresh_process":true}),
                "not independently reopened"
            );
        }
    }
    ensure!(
        rows[rows.len() - 2] == json!({"cleanup":true,"cases":count,"source_unchanged":true}),
        "unretired cases or modified source"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures() -> Value {
        serde_json::from_str(include_str!("../tests/resource-oracles.json")).unwrap()
    }
    #[test]
    fn digest_replays_preserve_the_frozen_independent_oracle() {
        assert_eq!(
            fixed_digests(),
            [
                2395918955545059960,
                10593364300825519773,
                13346994205906133797
            ]
        );
        assert_eq!(digests(0, false)[2], 6765992199539664772);
        assert_ne!(digests(4, false)[2], digests(4, true)[2]);
        assert_eq!(fnv(b"hello".iter().copied()), 0xa430d84680aabd0b);
        assert_eq!(
            fnv([1, 1].into_iter().flat_map(|epoch| payload(epoch, 8192))),
            11582251074155391637
        );
    }

    #[test]
    fn frozen_envelope_rows_reject_semantic_corruption() {
        let rows: Vec<Value> =
            serde_json::from_str(include_str!("../tests/resource-envelope-rows.json")).unwrap();
        envelope(&rows).unwrap();
        for (field, value) in [
            ("oracle_digest", 0),
            ("current_count", 2),
            ("live_bytes", 1_u64 << 40),
            ("retained_count", 63),
        ] {
            let mut corrupt = rows.clone();
            corrupt[1][field] = json!(value);
            assert!(envelope(&corrupt).is_err(), "{field}");
        }
        assert!(envelope(&rows[..rows.len() - 1]).is_err());
    }

    #[test]
    fn complete_compound_profiles_reject_wrong_counts_bytes_caps_and_outcomes() {
        let fixture = fixtures();
        let source = fixture["source"].as_str().unwrap();
        for profile in ["native", "inspect", "faults"] {
            let rows = fixture["compound"][profile].as_array().unwrap();
            compound(rows, profile, source).unwrap();
            for partial in [&rows[..rows.len() - 1], &rows[1..]] {
                assert!(compound(partial, profile, source).is_err());
            }
            let mut extra = rows.clone();
            extra.push(rows.last().unwrap().clone());
            assert!(compound(&extra, profile, source).is_err());
            let last = rows.len() - 1;
            for (index, key, value) in [
                (0, "digests", json!([0, 0, 0, 0])),
                (0, "revision", json!(0)),
                (0, "roots", json!(63)),
                (0, "peak_bytes", json!(CORE + 1)),
                (0, "file_bytes", json!(1)),
                (0, "elapsed_ns", json!(true)),
                (last, "source_id", json!("wrong")),
                (last, "complete", json!(1)),
                (last, "abi", json!(true)),
            ] {
                let mut bad = rows.clone();
                bad[index][key] = value;
                assert!(compound(&bad, profile, source).is_err(), "{profile}/{key}");
            }
            let details = match profile {
                "native" => vec![
                    (1, "prepare_ns", json!(vec![1; 7])),
                    (4, "commit_ns", json!(vec![1; 31])),
                    (4, "reader_ns", json!(vec![1; 255])),
                    (3, "baseline_live", json!(1)),
                ],
                "faults" => vec![
                    (0, "reopened_revision", json!(0)),
                    (0, "pressure_live_bytes", json!(1)),
                    (0, "pending_drafts", json!(1)),
                    (0, "error", json!("Ok")),
                ],
                _ => vec![],
            };
            for (index, key, value) in details {
                let mut bad = rows.clone();
                bad[index]["detail"][key] = value;
                assert!(compound(&bad, profile, source).is_err(), "{profile}/{key}");
            }
            if profile == "native" {
                for (key, value) in [("live_bytes", CORE / 2), ("file_bytes", 63 * MIB + 4096)] {
                    let mut bad = rows.clone();
                    bad[2][key] = json!(value);
                    assert!(compound(&bad, profile, source).is_err());
                }
            }
        }
    }
    #[test]
    fn complete_resilience_matrix_rejects_omitted_cases_wrong_roots_and_wrong_cut_edges() {
        let fixture = fixtures();
        let source = fixture["source"].as_str().unwrap();
        for (label, rows) in fixture["resilience"].as_object().unwrap() {
            let rows = rows.as_array().unwrap();
            resilience(rows, label, source).unwrap();
            let count = rows.len();
            for partial in [&[][..], &rows[..count - 1], &rows[1..]] {
                assert!(resilience(partial, label, source).is_err());
            }
            let mut bad = rows.clone();
            bad.reverse();
            assert!(resilience(&bad, label, source).is_err());
            let mut extra = rows.clone();
            extra.push(rows.last().unwrap().clone());
            assert!(resilience(&extra, label, source).is_err());
            let maintenance = label.starts_with("maintenance-");
            let index = if maintenance { 2 } else { 0 };
            for (i, key, value) in [
                (count - 1, "source_id", json!("wrong")),
                (count - 1, "features", json!(4.0)),
                (count - 1, "complete", json!(1)),
                (count - 2, "source_unchanged", json!(false)),
                (count - 2, "cases", json!(18.0)),
                (index, "revision", json!(1)),
                (index, "roots", json!(63)),
                (index, "roots", json!(64.0)),
                (index, "seq", json!(4.0)),
                (index, "digests", json!([0, 0, 0, 0])),
                (index, "peak_bytes", json!(CORE + 1)),
            ] {
                let mut bad = rows.clone();
                bad[i][key] = value;
                assert!(resilience(&bad, label, source).is_err(), "{label}/{key}");
            }
            if maintenance {
                for (key, value) in [
                    ("signal", json!(15)),
                    ("exchanged", json!(!rows[1]["exchanged"].as_bool().unwrap())),
                    ("occurrence", json!(100)),
                    ("orphans", json!(2)),
                    ("after", json!(0)),
                ] {
                    let mut bad = rows.clone();
                    bad[1][key] = value;
                    assert!(resilience(&bad, label, source).is_err(), "{label}/{key}");
                }
                let mut bad = rows.clone();
                bad[4]["count"] = json!(11);
                assert!(resilience(&bad, label, source).is_err());
                let mut bad = rows.clone();
                bad[2]["detail"]["fresh_process"] = json!(1);
                assert!(resilience(&bad, label, source).is_err());
            } else {
                for (key, value) in [
                    ("recovery_error", json!("Busy")),
                    ("acknowledged_floor", json!(999)),
                    ("initial", json!(-1)),
                    ("persist", json!(0)),
                ] {
                    let mut bad = rows.clone();
                    bad[0]["detail"][key] = value;
                    assert!(resilience(&bad, label, source).is_err(), "{label}/{key}");
                }
            }
        }
        let pending = fixture["resilience"]["recovery-2"].as_array().unwrap();
        let revisions = pending[..36]
            .iter()
            .step_by(2)
            .map(|r| r["revision"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            revisions,
            [
                212, 213, 212, 213, 212, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213, 213,
                212, 213
            ]
        );
        assert!(
            fixture["resilience"]["recovery-5"].as_array().unwrap()[..36]
                .iter()
                .step_by(2)
                .all(|r| r["revision"] == 213)
        );
        assert_eq!(
            fixture["resilience"]["maintenance-publish"][13]["exchanged"],
            false
        );
        assert_eq!(
            fixture["resilience"]["maintenance-publish"][16]["exchanged"],
            true
        );
    }
}
