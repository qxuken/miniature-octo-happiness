//! Exhaust a bounded APFS image around the existing exact-dylib C fixture.
use super::observes_library;
use crate::{
    delivered_faults, delivery, macos_volume,
    maintenance_space::bounded_fill,
    support::{path, sha},
    trace_process::{call, write_proof},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::Path,
};

pub(super) fn run(
    output: &Path,
    library: &Path,
    source: &str,
    env: &BTreeMap<String, String>,
    proof: &mut Value,
) -> Result<Value> {
    let logs = output.join("apfs-exhaustion");
    fs::create_dir(&logs)?;
    proof["host_volume"] = macos_volume::host_volume(&logs)?;
    let volume_env = BTreeMap::from([("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into())]);
    let mut clean: BTreeMap<OsString, OsString> =
        env.iter().map(|(k, v)| (k.into(), v.into())).collect();
    clean.insert("DYLD_PRINT_LIBRARIES".into(), "1".into());
    let shim = output.join("faults.dylib");
    let mut observed = clean.clone();
    observed.insert("DYLD_INSERT_LIBRARIES".into(), path(&shim));
    let mut outcome = None;
    let result = macos_volume::with_volume(proof, &logs, &volume_env, |proof, data| {
        let mut pids = BTreeSet::new();
        let mut phase =
            |proof: &mut Value, label: &str, action: &str, mode_argument: bool, observing: bool| {
                let mut argv = vec![
                    path(output.join("consumer")),
                    action.into(),
                    path(data),
                    source.into(),
                ];
                if mode_argument {
                    argv.push("5".into());
                }
                let report = call(
                    proof,
                    &logs,
                    argv,
                    &logs,
                    label,
                    if observing { &observed } else { &clean },
                    false,
                )?;
                ensure!(
                    pids.insert(report.pid.context("consumer PID missing")?),
                    "APFS phases require distinct processes"
                );
                let loader = fs::read_to_string(logs.join(format!("{label}.stderr")))?;
                ensure!(
                    observes_library(&loader, library, true)
                        && observes_library(&loader, &shim, true) == observing,
                    "exact APFS consumer loader/injector mismatch at {label}"
                );
                delivery::read_json(&logs.join(format!("{label}.stdout")))
            };

        let seed = phase(proof, "seed", "seed", false, false)?;
        delivered_faults::full_space_phase(&seed, false)?;
        let database = data.join("database.wdb");
        let seeded = sha(&database)?;
        let ballast = data.join("ballast");
        let (filled, free_before) = bounded_fill(data, &ballast, true)?;
        let exercise = phase(proof, "exercise", "5", false, true)?;
        let (before_ballast, before_reopen) = bounded_fill(data, &ballast, false)?;
        let failed = sha(&database)?;
        let full = phase(proof, "full", "full", true, false)?;
        delivered_faults::full_space_phase(&full, true)?;
        let (after_ballast, after_reopen) = bounded_fill(data, &ballast, false)?;
        ensure!(
            before_ballast == after_ballast && sha(&database)? == failed,
            "fresh process altered exhausted database or made additional space writable"
        );
        fs::remove_file(&ballast)?;
        let freed = macos_volume::space(data)?;
        ensure!(
            freed > 1024 * 1024,
            "ballast removal did not restore append headroom"
        );
        let retry = phase(proof, "retry", "recover", true, false)?;
        outcome = Some(delivered_faults::combine(&exercise, &retry, 5)?);
        let retry_hash = sha(&database)?;
        let archive = logs.join("database.wdb");
        fs::copy(&database, &archive)?;
        ensure!(
            retry_hash != failed && sha(&archive)? == retry_hash,
            "archived retry did not capture new revision"
        );
        proof["phases"] = json!({"seed":seed,"exercise":exercise,"full":full,"retry":retry});
        proof["space"] = json!({"initial_ballast_bytes":filled,"reported_available_before_failure":free_before,
            "reported_available_before_reopen":before_reopen,"reported_available_after_reopen":after_reopen,
            "reported_available_after_removal":freed,"failed_ballast_write_errno":libc::ENOSPC});
        proof["database_sha256"] =
            json!({"seed":seeded,"failed_and_full_reopen":failed,"retry":retry_hash});
        proof["consumer_pids"] = json!(pids);
        Ok(())
    });
    proof["verified"] = json!(result.is_ok());
    if let Err(error) = &result {
        proof["failure"] = json!(format!("{error:#}"));
    }
    write_proof(&logs, proof)?;
    result?;
    outcome.context("missing APFS fault result")
}
