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
    let [bulk, deep, retained] = fixed_digests();
    let writers = fnv((0..8).flat_map(|w| {
        let actual = seq + u64::from(changed && w == 0);
        actual
            .to_le_bytes()
            .into_iter()
            .chain(payload(1000 + w * 10 + actual, 65536))
    }));
    [bulk, deep, writers, retained]
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

#[cfg(test)]
mod tests {
    use super::*;
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
    }
}
