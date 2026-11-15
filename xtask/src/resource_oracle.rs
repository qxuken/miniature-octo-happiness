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
    let generate = || {
        let [bulk, deep, retained] = fixed_digests();
        let writers = fnv((0..8).flat_map(|w| {
            let actual = seq + u64::from(changed && w == 0);
            actual
                .to_le_bytes()
                .into_iter()
                .chain(payload(1000 + w * 10 + actual, 65536))
        }));
        [bulk, deep, writers, retained]
    };
    // Only the fixed short-workload states are used throughout the fault suites.
    if !matches!(seq, 0 | 4) {
        assert!(seq <= 100);
        return generate();
    }
    static STATES: [OnceLock<[u64; 4]>; 4] = [const { OnceLock::new() }; 4];
    let index = usize::from(seq == 4) * 2 + usize::from(changed);
    *STATES[index].get_or_init(generate)
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
    expected_digests: [u64; 4],
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
        row["digests"] == json!(expected_digests),
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
    state_at(
        row,
        phase,
        revision,
        4,
        changed,
        60 * MIB,
        digests(4, changed),
    )?;
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
        state_at(
            row,
            phase,
            revision,
            seq,
            changed,
            64 * MIB * 9 / 10,
            digests(seq, changed),
        )?;
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
            "contended_commits" => contention_samples(detail)?,
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

fn contention_samples(detail: &Value) -> Result<()> {
    ensure!(
        detail["writers"] == 8
            && detail["rounds"] == 4
            && integers(&detail["commit_ns"], 32)
            && integers(&detail["reader_ns"], 256),
        "incomplete concurrent samples"
    );
    Ok(())
}

#[cfg(unix)]
const BREADTH_PHASES: [&str; 18] = [
    "near_cap",
    "image_limit",
    "reopen",
    "backup",
    "restore",
    "compact",
    "overlapping_drafts",
    "concurrent_limit",
    "concurrent_retry",
    "concurrent_reopen",
    "acknowledged",
    "slot_drop",
    "witness_drop",
    "append_torn",
    "slot_before_checksum",
    "slot_through_checksum",
    "witness_before_checksum",
    "witness_through_checksum",
];

#[cfg(unix)]
fn breadth_digest(current_revision: u64) -> u64 {
    breadth_digest_with(current_revision, |key| match key {
        0..1024 if current_revision >= 134 => Some(65 << 32 | key),
        2048..3072 if current_revision == 135 => Some(0x5348_4152_4544),
        _ => None,
    })
}

#[cfg(unix)]
fn breadth_digest_with(current_revision: u64, current: impl Fn(u64) -> Option<u64> + Copy) -> u64 {
    fnv((0..=64).flat_map(|id| {
        let revision: u64 = if id == 0 {
            current_revision
        } else {
            4 + 2 * id
        };
        let epoch = if id == 0 { 64 } else { id };
        revision
            .to_le_bytes()
            .into_iter()
            .chain((0..4095u64).flat_map(move |key| {
                let regular_update = key.checked_sub(1024).filter(|n| (10..=epoch).contains(n));
                let version = if key < 1024 {
                    epoch.min(9)
                } else {
                    regular_update.unwrap_or(0)
                };
                let bits = if id == 0 { current(key) } else { None }.unwrap_or(version << 32 | key);
                bits.to_le_bytes()
            }))
            .chain((0..8192u32).flat_map(|entry| {
                (entry % 4096)
                    .to_be_bytes()
                    .into_iter()
                    .chain(0x5749_4445u64.to_le_bytes())
            }))
    }))
}

#[cfg(unix)]
fn sustained_digest(cycle: u64) -> u64 {
    breadth_digest_with(133 + 48 * cycle, |key| {
        (key < 3072).then(|| 0x5355_0000_0000_0000 | ((key / 1024) << 32) | (16 * cycle))
    })
}

#[cfg(unix)]
fn latency_samples(mut samples: Vec<u64>) -> Value {
    samples.sort_unstable();
    let rank = |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1];
    json!({
        "count":samples.len(), "zero_ticks":samples.partition_point(|&ns| ns == 0),
        "p50_ns":rank(50), "p95_ns":rank(95),
        "p99_ns":rank(99), "max_ns":rank(100)
    })
}

#[cfg(unix)]
pub fn sustained_breadth(rows: &[Value], source: &str) -> Result<Value> {
    ensure!(
        rows.len() == 10 && rows.last() == Some(&completion("breadth-sustained", source)),
        "incomplete sustained breadth cycles or wrong build identity"
    );
    let mut start_bytes = 58_523_648;
    ensure!(
        rows[0]
            == json!({"phase":"sustained_seed","revision":133,"digest":breadth_digest(133),"file_bytes":start_bytes}),
        "invalid sustained breadth seed"
    );
    let mut commits = Vec::new();
    let mut acquisitions = Vec::new();
    let mut transitions = Vec::new();
    let mut compactions = Vec::new();
    let mut reopens = Vec::new();
    let mut intermediate_revisions = 0;
    for (index, row) in rows[1..9].iter().enumerate() {
        let cycle = index as u64 + 1;
        let start = 133 + 48 * (cycle - 1);
        let end = start + 48;
        let written = number(row, "file_bytes")?;
        ensure!(
            row["phase"] == "sustained_cycle"
                && row["cycle"] == cycle
                && row["revision"] == end
                && row["digest"] == sustained_digest(cycle)
                && (60 * MIB..=64 * MIB).contains(&written)
                && written == start_bytes + 48 * 126_976
                && (1..=CORE).contains(&number(row, "core_peak_bytes")?),
            "wrong sustained state or file growth in cycle {cycle}"
        );
        let compact = number(row, "compact_bytes")?;
        ensure!(compact == 58_654_720, "bad sustained compacted size");
        start_bytes = compact;
        let wall = number(row, "workload_ns")?;
        let maintenance = number(row, "maintenance_ns")?;
        let reopen = number(row, "reopen_ns")?;
        ensure!(maintenance > 0 && reopen > 0, "missing maintenance timing");
        compactions.push(maintenance);
        reopens.push(reopen);

        let writers: Vec<Vec<[u64; 2]>> = serde_json::from_value(row["writers"].clone())?;
        let readers: Vec<Vec<[u64; 5]>> = serde_json::from_value(row["readers"].clone())?;
        ensure!(
            writers.len() == 3 && readers.len() == 4,
            "missing worker samples"
        );
        let mut order = Vec::new();
        for (writer, samples) in writers.iter().enumerate() {
            ensure!(
                samples.len() == 16 && samples.windows(2).all(|w| w[0][0] < w[1][0]),
                "writer receipts out of order"
            );
            for &[revision, ns] in samples {
                ensure!((1..=wall).contains(&ns), "invalid writer timing");
                commits.push(ns);
                order.push((revision, writer));
            }
        }
        order.sort_unstable();
        ensure!(
            order.iter().map(|p| p.0).eq(start + 1..=end),
            "wrong committed revision order"
        );
        let mut sequences = [16 * (cycle - 1); 3];
        let mut states = vec![sequences];
        for (_, writer) in order {
            sequences[writer] += 1;
            states.push(sequences);
        }
        let mut seen = std::collections::BTreeSet::new();
        for samples in readers {
            ensure!(
                (2..=4096).contains(&samples.len())
                    && samples[0][0] == start
                    && samples.last().unwrap()[0] == end,
                "reader did not bracket the writer window"
            );
            let mut previous = start;
            for (i, [revision, ns, a, b, c]) in samples.into_iter().enumerate() {
                ensure!(
                    (previous..=end).contains(&revision),
                    "reader revision out of order"
                );
                // An uncontended acquisition can start and end in one clock tick.
                ensure!(ns <= wall, "reader timing exceeds the writer window");
                ensure!(
                    states[(revision - start) as usize] == [a, b, c],
                    "reader observed a state inconsistent with the acknowledged order"
                );
                if i != 0 {
                    acquisitions.push(ns);
                    if revision != previous {
                        transitions.push(ns);
                    }
                    if (start + 1..end).contains(&revision) {
                        seen.insert(revision);
                    }
                }
                previous = revision;
            }
        }
        ensure!(
            !seen.is_empty(),
            "cycle has no intermediate reader observations"
        );
        intermediate_revisions += seen.len();
    }
    Ok(json!({
        "commits":latency_samples(commits),
        "snapshot_acquisition":latency_samples(acquisitions),
        "revision_transition_acquisition":latency_samples(transitions),
        "compaction":latency_samples(compactions),
        "reopen":latency_samples(reopens),
        "intermediate_revisions":intermediate_revisions
    }))
}

#[cfg(unix)]
pub fn compound_breadth(rows: &[Value], source: &str) -> Result<()> {
    ensure!(
        rows.len() == BREADTH_PHASES.len() + 1
            && rows.last() == Some(&completion("breadth", source)),
        "incomplete breadth phases or wrong build identity"
    );
    let [old_digest, first_digest, final_digest] = [133, 134, 135].map(breadth_digest);
    let source_bytes = number(&rows[0], "file_bytes")?;
    let compact = number(&rows[3], "file_bytes")?;
    let appended = number(&rows[7], "file_bytes")?;
    let retried = number(&rows[8], "file_bytes")?;
    ensure!(
        (64 * MIB - 4 * MIB + 1..=64 * MIB).contains(&source_bytes)
            && (1..source_bytes).contains(&compact)
            && (compact + 4 * MIB..64 * MIB).contains(&appended)
            && appended - compact > 64 * MIB - appended
            && (appended + 1..=64 * MIB).contains(&retried),
        "bad breadth source, compacted or appended image size"
    );
    for (index, (row, phase)) in rows.iter().zip(BREADTH_PHASES).enumerate() {
        ensure!(row["phase"] == phase, "wrong breadth phase: {phase}");
        if phase == "overlapping_drafts" {
            let draft_live = number(row, "draft_live_bytes")?;
            let reading = number(row, "reader_ns")?;
            ensure!(
                row["digest"] == old_digest
                    && row["drafts"] == 3
                    && row["writes_per_draft"] == 1024
                    && (1..=CORE).contains(&draft_live)
                    && draft_live > number(row, "baseline_live_bytes")?
                    && (1..=reading).contains(&number(row, "acquire_ns")?)
                    && number(row, "first_ns")? >= reading
                    && number(row, "conflict_ns")? >= reading,
                "wrong reader state or missing draft overlap"
            );
            continue;
        }
        let (bytes, witness, revision, digest) = match phase {
            "concurrent_limit" => (appended, None, 134, first_digest),
            "concurrent_retry" | "concurrent_reopen" => (retried, None, 135, final_digest),
            "acknowledged" | "witness_through_checksum" => {
                (appended, Some(true), 134, first_digest)
            }
            "slot_drop" | "slot_before_checksum" => (appended, Some(true), 133, old_digest),
            "witness_drop" | "slot_through_checksum" | "witness_before_checksum" => {
                (appended, Some(false), 134, first_digest)
            }
            "append_torn" => (compact + 4096, Some(true), 133, old_digest),
            _ => (
                if index < 3 { source_bytes } else { compact },
                None,
                133,
                old_digest,
            ),
        };
        ensure!(
            row["revision"] == revision
                && row["roots"] == 64
                && row["width"] == 4096
                && row["depth"] == 64
                && row["digest"] == digest
                && row["file_bytes"] == bytes
                && row.get("inspected_witness") == Some(&json!(witness))
                && (1..=CORE).contains(&number(row, "core_peak_bytes")?)
                && number(row, "rss_kib")? > 0
                && number(row, "elapsed_ns")? > 0,
            "wrong breadth state or missing measurement: {phase}"
        );
    }
    Ok(())
}

#[cfg(unix)]
pub fn compound_cycles(rows: &[Value], source: &str, isolated: bool) -> Result<()> {
    const PHASES: [&str; 7] = [
        "cycle_open",
        "contended_commits",
        "image_limit_rejected",
        "cycle_backup_rejected",
        "cycle_backup",
        "cycle_restore",
        "cycle_compact",
    ];
    let (terminal, events) = rows.split_last().context("missing cycle completion")?;
    ensure!(
        *terminal == completion("cycles", source) && events.len() == 24 * PHASES.len(),
        "incomplete cycle sequence or wrong build identity"
    );
    let mut compacted = None;
    let mut before = digests(4, false);
    let (cycles, _) = events.as_chunks::<{ PHASES.len() }>();
    for (index, cycle) in cycles.iter().enumerate() {
        let revision = 212 + 32 * index as u64;
        let seq = 4 + 4 * index as u64;
        let after = digests(seq + 4, false);
        let written = number(&cycle[1], "file_bytes")?;
        for (i, (row, phase)) in cycle.iter().zip(PHASES).enumerate() {
            let advance = u64::from(i != 0);
            state_at(
                row,
                phase,
                revision + 32 * advance,
                seq + 4 * advance,
                false,
                60 * MIB,
                if i == 0 { before } else { after },
            )?;
            let bytes = number(row, "file_bytes")?;
            let detail = &row["detail"];
            match phase {
                "cycle_open" => ensure!(
                    *detail == json!({"cycle": index + 1})
                        && compacted.is_none_or(|previous| previous == bytes),
                    "cycle did not resume the compacted file"
                ),
                "contended_commits" => {
                    contention_samples(detail)?;
                    ensure!(
                        written > 64 * MIB * 96 / 100,
                        "workload did not reach the image cap"
                    );
                }
                "image_limit_rejected" => ensure!(
                    bytes == written
                        && *detail == json!({"error":"Limit","new_payload_bytes":4*MIB}),
                    "wrong image-limit rejection"
                ),
                _ => {
                    let mut expected = if phase == "cycle_backup_rejected" {
                        ensure!(bytes == written, "wrong backup-budget rejection");
                        json!({"copy_budget":CORE,"error":"Limit"})
                    } else {
                        ensure!(bytes < written, "maintenance did not reclaim space");
                        json!({"copy_budget":256*MIB})
                    };
                    if isolated {
                        let peak = number(detail, "maintenance_peak_rss_bytes")?;
                        ensure!(peak > 0, "missing maintenance peak RSS");
                        expected["maintenance_peak_rss_bytes"] = json!(peak);
                    }
                    ensure!(*detail == expected, "wrong maintenance detail");
                }
            }
        }
        compacted = Some(number(&cycle[6], "file_bytes")?);
        before = after;
    }
    Ok(())
}

/// The finite single-lane resource driver emits telemetry plus an independent
/// digest of the application payloads. Check its order and each root's contents.
#[cfg(unix)]
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

/// Check adapter diagnostics and recovered data against the public operation
/// order. Each failure is followed by a separate reopen and retry checker.
#[cfg(unix)]
pub fn namespace_failures(rows: &[Value], source: &str, macos: bool) -> Result<()> {
    const PROFILE: &str = "namespace-failures";
    let quota = if macos { 69 } else { 122 };
    let schedule = [
        (1, 28, "Exchange", "DiskFull", false, true, false),
        (2, quota, "Sync", "Quota", true, true, false),
        (3, 5, "Sync", "Other", true, true, false),
        (4, 28, "Remove", "DiskFull", true, true, false),
        (5, quota, "Sync", "Quota", true, false, false),
        (6, 5, "Sync", "Other", true, false, false),
        (1, 5, "Exchange", "Other", true, true, false),
        (1, 13, "Exchange", "Permission", false, true, true),
        (4, 13, "Remove", "Permission", true, true, true),
    ];
    ensure!(
        rows.len() == 2 + 3 * schedule.len() + 2
            && rows.last() == Some(&completion(PROFILE, source))
            && rows[1] == json!({"disabled_injection_rejected":true,"calibrated_calls":6}),
        "incomplete namespace reports or missing injector control"
    );
    let fresh = json!({"fresh_process":true});
    state(&rows[0], "namespace_control", 212, false, Some(63_074_304))?;
    ensure!(
        rows[0]["detail"] == fresh,
        "missing fresh namespace control"
    );
    for (
        (index, (calls, raw, operation, category, exchanged, orphan, kernel)),
        [fail, reopen, retry],
    ) in schedule
        .into_iter()
        .enumerate()
        .zip(rows[2..].as_chunks::<3>().0)
    {
        let mode = index + 1;
        ensure!(
            *fail
                == json!({"mode":mode,"calls":calls,
                "raw":raw,"operation":operation,"category":category,
                "error":"OutcomeUnknown","exchanged":exchanged,"orphan":orphan,"kernel_denial":kernel}),
            "namespace diagnostic mismatch in case {mode}"
        );
        state(
            reopen,
            &format!("namespace_reopen_{mode}"),
            212,
            false,
            Some(if exchanged { 63_074_304 } else { 66_154_496 }),
        )?;
        state(
            retry,
            &format!("namespace_retry_{mode}"),
            212,
            false,
            Some(63_074_304),
        )?;
        ensure!(
            reopen["detail"] == fresh && retry["detail"] == fresh,
            "missing fresh namespace reopen or retry"
        );
    }
    ensure!(
        rows[rows.len() - 2]
            == json!({"cleanup":true,"cases":schedule.len(),"source_unchanged":true}),
        "namespace case cleanup incomplete"
    );
    Ok(())
}

/// Independent schedule/outcome oracle for the injected second failures and
/// the killed native maintenance worker on both sides of selected boundaries.
#[cfg(unix)]
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

#[cfg(all(test, unix))]
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
    fn breadth_oracle_requires_exact_roots_and_recovery_outcomes() {
        const OLD: u64 = 7_918_828_611_224_877_843;
        const FIRST: u64 = 12_349_113_872_356_617_204;
        const RETRIED: u64 = 11_331_111_483_656_711_877;
        let mut rows: Vec<_> = BREADTH_PHASES
            .iter()
            .enumerate()
            .map(|(i, phase)| {
                let (revision, digest, witness, bytes) = match i {
                    0..3 => (133, OLD, None, 63_574_016),
                    3..6 => (133, OLD, None, 58_523_648),
                    6 => {
                        return json!({
                            "phase":phase,"digest":OLD,"drafts":3,"writes_per_draft":1024,
                            "baseline_live_bytes":79_174_728,"draft_live_bytes":79_862_856,
                        "acquire_ns":1,"reader_ns":12,"first_ns":36,"conflict_ns":21,
                        });
                    }
                    7 => (134, FIRST, None, 62_840_832),
                    8 | 9 => (135, RETRIED, None, 62_967_808),
                    10 | 17 => (134, FIRST, Some(true), 62_840_832),
                    11 | 14 => (133, OLD, Some(true), 62_840_832),
                    13 => (133, OLD, Some(true), 58_527_744),
                    _ => (134, FIRST, Some(false), 62_840_832),
                };
                json!({
                    "phase":phase,"revision":revision,"roots":64,
                    "width":4096,"depth":64,"digest":digest,"file_bytes":bytes,
                    "elapsed_ns":1,"core_peak_bytes":1,"rss_kib":1,
                    "inspected_witness":witness
                })
            })
            .collect();
        rows.push(completion("breadth", "fixture"));
        compound_breadth(&rows, "fixture").unwrap();
        assert!(compound_breadth(&rows[1..], "fixture").is_err());
        assert!(compound_breadth(&rows, "different-build").is_err());
        for (index, field, replacement) in [
            (1, "file_bytes", json!(63_574_017)),
            (3, "roots", json!(63)),
            (4, "depth", json!(63)),
            (5, "digest", json!(0)),
            (6, "drafts", json!(2)),
            (6, "acquire_ns", json!(13)),
            (6, "conflict_ns", json!(11)),
            (6, "baseline_live_bytes", json!(u64::MAX)),
            (9, "revision", json!(134)),
            (13, "file_bytes", json!(58_523_648)),
            (14, "revision", json!(134)),
            (16, "inspected_witness", json!(true)),
        ] {
            let mut bad = rows.clone();
            bad[index][field] = replacement;
            assert!(compound_breadth(&bad, "fixture").is_err(), "{field}");
        }
    }

    #[test]
    fn sustained_breadth_replays_reader_states_from_writer_receipts() {
        let mut rows = vec![json!({"phase":"sustained_seed","revision":133,
                "digest":7_918_828_611_224_877_843u64,"file_bytes":58_523_648})];
        for cycle in 1..=8u64 {
            let start = 133 + (cycle - 1) * 48;
            let before = 16 * (cycle - 1);
            let after = before + 16;
            let writers: Vec<_> = (0..3)
                .map(|writer| {
                    (1..=16)
                        .map(|i| [start + writer * 16 + i, 11])
                        .collect::<Vec<_>>()
                })
                .collect();
            let reader = [
                [start, 13, before, before, before],
                [start + 1, 0, before + 1, before, before],
                [start + 1, 9, before + 1, before, before],
                [start + 48, 8, after, after, after],
            ];
            rows.push(json!({
                "phase":"sustained_cycle","cycle":cycle,"revision":start+48,"digest":sustained_digest(cycle),
                "file_bytes":if cycle==1 {64_618_496} else {64_749_568},
                "compact_bytes":58_654_720,"core_peak_bytes":CORE,
                "workload_ns":20,"maintenance_ns":19,"reopen_ns":16,"writers":writers,"readers":([reader;4]),
            }));
        }
        rows.push(completion("breadth-sustained", "sample"));
        let summary = sustained_breadth(&rows, "sample").unwrap();
        assert_eq!(summary["commits"]["count"], 384);
        assert_eq!(summary["snapshot_acquisition"]["count"], 96);
        assert_eq!(summary["snapshot_acquisition"]["zero_ticks"], 32);
        assert_eq!(summary["snapshot_acquisition"]["p99_ns"], 9);
        assert_eq!(summary["revision_transition_acquisition"]["count"], 64);
        assert_eq!(summary["revision_transition_acquisition"]["p99_ns"], 8);
        assert!(sustained_breadth(&rows[..9], "sample").is_err());

        let mut bad = rows.clone();
        bad[1]["readers"][0][1][2] = json!(0);
        assert!(sustained_breadth(&bad, "sample").is_err());
        bad = rows.clone();
        bad[8]["writers"][0][0][0] = bad[8]["writers"][1][0][0].clone();
        assert!(sustained_breadth(&bad, "sample").is_err());
        bad = rows.clone();
        for reader in bad[1]["readers"].as_array_mut().unwrap() {
            reader.as_array_mut().unwrap().drain(1..3);
        }
        assert!(sustained_breadth(&bad, "sample").is_err());
    }

    #[test]
    fn compound_cycle_oracle_rejects_missing_work_and_stale_files() {
        let mut rows = Vec::new();
        let phases = [
            ("cycle_open", 61 * MIB, json!({})),
            (
                "contended_commits",
                63 * MIB,
                json!({
                    "writers":8,"rounds":4,"commit_ns":vec![1;32],"reader_ns":vec![1;256]
                }),
            ),
            (
                "image_limit_rejected",
                63 * MIB,
                json!({"error":"Limit","new_payload_bytes":4*MIB}),
            ),
            (
                "cycle_backup_rejected",
                63 * MIB,
                json!({"error":"Limit","copy_budget":CORE}),
            ),
            ("cycle_backup", 61 * MIB, json!({"copy_budget":256*MIB})),
            ("cycle_restore", 61 * MIB, json!({"copy_budget":256*MIB})),
            ("cycle_compact", 61 * MIB, json!({"copy_budget":256*MIB})),
        ];
        for cycle in 1..=24_u64 {
            let before = digests(4 * cycle, false);
            let after = digests(4 * (cycle + 1), false);
            for (index, (phase, bytes, detail)) in phases.iter().enumerate() {
                let completed = cycle - u64::from(index == 0);
                let seq = 4 + 4 * completed;
                let detail = if index == 0 {
                    json!({"cycle":cycle})
                } else {
                    detail.clone()
                };
                rows.push(json!({
                    "phase":phase,"revision":212+32*completed,"seq":seq,"changed":false,
                    "roots":64,"digests":if index == 0 { before } else { after },"live_bytes":1,"peak_bytes":1,
                    "cap_bytes":CORE,"rss_kib":1,"file_bytes":bytes,"elapsed_ns":1,
                    "detail":detail,
                }));
            }
        }
        rows.push(completion("cycles", "fixture"));
        compound_cycles(&rows, "fixture", false).unwrap();
        assert!(compound_cycles(&rows[1..], "fixture", false).is_err());
        assert!(compound_cycles(&rows, "fixture", true).is_err());
        for (index, field, value) in [
            (
                1,
                "detail",
                json!({"writers":8,"rounds":4,"commit_ns":vec![1;31],"reader_ns":vec![1;256]}),
            ),
            (2, "file_bytes", json!(62 * MIB)),
            (3, "revision", json!(212)),
            (4, "digests", json!(digests(4, false))),
            (7, "file_bytes", json!(62 * MIB)),
        ] {
            let mut bad = rows.clone();
            bad[index][field] = value;
            assert!(
                compound_cycles(&bad, "fixture", false).is_err(),
                "{index}/{field}"
            );
        }
        for row in rows.iter_mut().take(24 * 7).filter(|row| {
            let phase = row["phase"].as_str().unwrap();
            phase.starts_with("cycle_") && phase != "cycle_open"
        }) {
            row["detail"]["maintenance_peak_rss_bytes"] = json!(1);
        }
        compound_cycles(&rows, "fixture", true).unwrap();
        assert!(compound_cycles(&rows, "fixture", false).is_err());
        rows[3]["detail"]["maintenance_peak_rss_bytes"] = json!(0);
        assert!(compound_cycles(&rows, "fixture", true).is_err());
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
    fn namespace_oracle_requires_the_native_error_and_both_fresh_checks() {
        let fixture = fixtures();
        let source = fixture["source"].as_str().unwrap();
        let status = &fixture["resilience"]["maintenance-publish"][0];
        let state = |phase: String, bytes: u64| {
            let mut state = status.clone();
            state["phase"] = json!(phase);
            state["file_bytes"] = json!(bytes);
            state
        };
        let mut rows = vec![
            state("namespace_control".into(), 63_074_304),
            json!({"disabled_injection_rejected":true,"calibrated_calls":6}),
        ];
        for (i, (calls, raw, operation, category, orphan)) in [
            (1, 28, "Exchange", "DiskFull", true),
            (2, 69, "Sync", "Quota", true),
            (3, 5, "Sync", "Other", true),
            (4, 28, "Remove", "DiskFull", true),
            (5, 69, "Sync", "Quota", false),
            (6, 5, "Sync", "Other", false),
            (1, 5, "Exchange", "Other", true),
            (1, 13, "Exchange", "Permission", true),
            (4, 13, "Remove", "Permission", true),
        ]
        .into_iter()
        .enumerate()
        {
            let mode = i + 1;
            let exchanged = !matches!(mode, 1 | 8);
            rows.push(json!({"mode":mode,"calls":calls,
                "raw":raw,"operation":operation,"category":category,
                "error":"OutcomeUnknown","exchanged":exchanged,"orphan":orphan,"kernel_denial":mode >= 8}));
            rows.push(state(
                format!("namespace_reopen_{mode}"),
                if exchanged { 63_074_304 } else { 66_154_496 },
            ));
            rows.push(state(format!("namespace_retry_{mode}"), 63_074_304));
        }
        rows.push(json!({"cleanup":true,"cases":9,"source_unchanged":true}));
        rows.push(completion("namespace-failures", source));
        namespace_failures(&rows, source, true).unwrap();
        assert!(namespace_failures(&rows, source, false).is_err());
        for (row, field, value) in [
            (1, "disabled_injection_rejected", json!(false)),
            (2, "error", json!("Io")),
            (2, "exchanged", json!(true)),
            (3, "file_bytes", json!(63_074_304)),
            (4, "detail", json!({"fresh_process":false})),
            (14, "orphan", json!(true)),
            (20, "calls", json!(7)),
            (23, "kernel_denial", json!(false)),
            (24, "file_bytes", json!(63_074_304)),
            (26, "calls", json!(1)),
            (29, "source_unchanged", json!(false)),
        ] {
            let mut broken = rows.clone();
            broken[row][field] = value;
            assert!(
                namespace_failures(&broken, source, true).is_err(),
                "{field}"
            );
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
