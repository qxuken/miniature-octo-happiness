//! Host-side oracles for kernel ENOSPC/EDQUOT, separate from the fixture.
use crate::{
    delivery,
    resource_oracle::{self, MIB, number},
    support::{cmd, path},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{ffi::OsString, path::Path};
pub const CHUNK: u64 = 8 * MIB;
pub const CEILING: u64 = 384 * MIB;
pub const LOG_BYTES: u64 = MIB;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Quota,
    Enospc,
}
impl Space {
    pub fn errno(self) -> i32 {
        if self == Self::Quota { 122 } else { 28 }
    }
    pub fn profile(self) -> &'static str {
        if self == Self::Quota {
            "quota"
        } else {
            "enospc"
        }
    }
    pub fn category(self) -> &'static str {
        if self == Self::Quota {
            "Quota"
        } else {
            "DiskFull"
        }
    }
}
pub fn identity(uid: u32) -> Value {
    json!({"uid":uid,"uids":([uid;4]),"gids":([uid;4]),"groups":[],"caps":{"CapInh":0,"CapPrm":0,"CapEff":0,"CapBnd":0,"CapAmb":0},"no_new_privs":1})
}
pub fn verify_probe(row: &Value, uid: u32, mode: &str, ceiling: u64, space: Space) -> Result<()> {
    ensure!(
        row["mode"] == mode && number(row, "pid")? > 0,
        "wrong probe identity"
    );
    ensure!(
        row["identity"] == identity(uid),
        "guest privileges not verified"
    );
    for field in ["global_free_bytes", "available_bytes"] {
        let bytes = number(row, field)?;
        ensure!(
            space != Space::Quota || bytes > 16 * MIB,
            "global space too low to distinguish quota"
        );
    }
    match mode {
        "fill" => ensure!(
            row["errno"] == space.errno()
                && row["ceiling"] == ceiling
                && number(row, "written")? <= ceiling,
            "actual bounded expected kernel space error required"
        ),
        "headroom" => ensure!(
            row["released_bytes"] == CHUNK,
            "exact owned headroom required"
        ),
        "remove" => ensure!(row["removed"] == true, "ballast removal not proven"),
        "write-control" => ensure!(
            row["fsynced_bytes"] == 4096,
            "same-UID fsynced success required"
        ),
        "identity" | "install-seed" => (),
        _ => anyhow::bail!("unknown guest probe mode"),
    }
    Ok(())
}
fn rows(revision: u64) -> Value {
    let new = revision == 4;
    let length = if new { 65536 } else { 512 };
    let (mult, add) = if new { (17, 23) } else { (13, 7) };
    const HEX: &[u8] = b"0123456789abcdef";
    let bytes = (0..length)
        .flat_map(|i| {
            let b = (i * mult + add) % 251;
            [HEX[b / 16], HEX[b % 16]]
        })
        .map(char::from)
        .collect::<String>();
    json!([{"key_hex":"7061796c6f6164","kind":"bytes","bytes_hex":bytes},{"key_hex":"726f77","kind":"integer","integer":if new{18}else{17}}])
}
fn fixture_expected(source: &str, stage: &str, revision: u64, pid: u64) -> Value {
    let fail = stage == "fail";
    json!({"stage":stage,"source":source,"features":0,"pid":pid,"current_revision":revision,"retained_id":1,"retained_revision":2,"commit_error":if fail{14}else{0},"native_category":if fail{"Quota"}else{"None"},"native_raw_code":fail.then_some(122),"native_operation":if fail{"Write"}else{"None"},"terminal_error":if fail{11}else{0},"recovery_required":fail,"blocked_write_error":if fail{16}else{0},"current_rows":rows(revision),"retained_rows":rows(3)})
}
pub fn fixture(row: &Value, source: &str, stage: &str, revision: u64, pid: u32) -> Result<()> {
    ensure!(
        matches!(stage, "seed" | "fail" | "inspect" | "retry")
            && matches!(revision, 3 | 4)
            && pid > 0,
        "invalid fixture expectation"
    );
    let expected = fixture_expected(source, stage, revision, u64::from(pid));
    ensure!(
        row.as_object()
            .is_some_and(|o| o.keys().eq(expected.as_object().unwrap().keys())),
        "fixture schema mismatch"
    );
    for (key, value) in expected.as_object().unwrap() {
        ensure!(&row[key] == value, "fixture oracle mismatch: {key}");
    }
    Ok(())
}
pub fn maintenance<'a>(
    rows: &'a [Value],
    mode: &str,
    label: &str,
    source: &str,
    space: Space,
    pid: u32,
) -> Result<&'a Value> {
    ensure!(
        rows.len() == 2 && rows[1] == resource_oracle::completion(mode, source),
        "incomplete/wrong maintenance build identity"
    );
    let row = &rows[0];
    if matches!(mode, "space-check" | "space-advance") {
        let changed = matches!(label, "advanced" | "advanced-source");
        resource_oracle::state(
            row,
            &format!("space_{label}"),
            212 + u64::from(changed),
            changed,
            None,
        )?;
        ensure!(
            row["detail"] == json!({"fresh_process":true}),
            "missing fresh checker"
        );
        return Ok(row);
    }
    ensure!(
        matches!(label, "backup" | "restore" | "compact") && pid > 0,
        "invalid maintenance expectation"
    );
    let expected = match mode {
        "space-fail" => {
            let partial = number(row, "partial_bytes")?;
            ensure!(
                (MIB..63_074_304).contains(&partial)
                    && matches!(row["native_operation"].as_str(), Some("Write" | "Sync")),
                "not a partial-copy exhaustion"
            );
            json!({"stage":"fail","operation":label,"pid":pid,"error":14,"category":space.category(),"raw":space.errno(),"native_operation":row["native_operation"],"partial_bytes":partial,"source_unchanged":true,"backup_unchanged":true})
        }
        "space-retry" => {
            json!({"stage":"retry","operation":label,"pid":pid,"partial_rejected":true,"existing_rejected":label!="compact","partial_preserved":true,"matches_backup":true})
        }
        "space-retire" => {
            json!({"stage":"retire","operation":label,"pid":pid,"owned_outputs_removed":true})
        }
        _ => anyhow::bail!("unknown maintenance mode"),
    };
    ensure!(
        *row == expected,
        "wrong maintenance status, disposition, PID or kernel classification"
    );
    Ok(row)
}
pub fn one_json(s: &str) -> Result<Value> {
    ensure!(s.len() <= LOG_BYTES as usize, "JSON result too large");
    let value = delivery::json(s.as_bytes())?;
    ensure!(value.is_object(), "result must be one JSON object");
    Ok(value)
}
pub fn owned_loops(output: &str, image: &Path) -> Result<Vec<String>> {
    let value = one_json(output)?;
    let mut owned = vec![];
    for row in value["loopdevices"]
        .as_array()
        .context("invalid loop inventory")?
    {
        if row["back-file"] == image.to_str().context("UTF-8 image path")? {
            let name = row["name"].as_str().context("loop name")?;
            ensure!(
                name.strip_prefix("/dev/loop")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
                "invalid owned loop name"
            );
            ensure!(
                !owned.iter().any(|n| n == name),
                "duplicate owned loop identity"
            );
            owned.push(name.to_owned());
        }
    }
    Ok(owned)
}
pub fn exact_mount(output: &str, mount: &Path, device: &str) -> Result<Value> {
    let info = one_json(output)?;
    let rows = info["filesystems"]
        .as_array()
        .context("invalid mount inventory")?;
    ensure!(rows.len() == 1, "one exact mount required");
    ensure!(
        rows[0]["target"] == mount.to_str().context("UTF-8 mount path")?
            && rows[0]["source"] == device
            && rows[0]["fstype"] == "ext4",
        "mount/backing identity mismatch"
    );
    Ok(rows[0].clone())
}
pub fn formatter(executable: &Path, image: &Path) -> Vec<OsString> {
    let mut args = vec![path(executable)];
    args.extend(cmd([
        "-t",
        "ext4",
        "-q",
        "-F",
        "-O",
        "quota",
        "-E",
        "quotatype=usrquota",
    ]));
    args.push(path(image));
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    const UID: u32 = 60000;
    const SOURCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    fn probe() -> Value {
        json!({"mode":"fill","pid":42,"identity":identity(UID),"global_free_bytes":32*MIB,"available_bytes":32*MIB,"errno":122,"written":4*MIB,"ceiling":CHUNK})
    }
    fn mutate(good: &Value, key: &str, value: Value) -> Value {
        let mut v = good.clone();
        v[key] = value;
        v
    }
    #[test]
    fn resolved_mke2fs_explicitly_selects_ext4() {
        assert_eq!(
            formatter(
                Path::new("/usr/sbin/mke2fs"),
                Path::new("/owned/image.ext4")
            ),
            cmd([
                "/usr/sbin/mke2fs",
                "-t",
                "ext4",
                "-q",
                "-F",
                "-O",
                "quota",
                "-E",
                "quotatype=usrquota",
                "/owned/image.ext4"
            ])
        );
    }
    #[test]
    fn mount_and_loop_require_exact_owned_paths() {
        let image = Path::new("/private/image.ext4");
        let point = Path::new("/private/mount");
        let loops = json!({"loopdevices":[{"name":"/dev/loop7","back-file":image}]}).to_string();
        assert_eq!(owned_loops(&loops, image).unwrap(), ["/dev/loop7"]);
        assert!(
            owned_loops(&loops, Path::new("/other/image.ext4"))
                .unwrap()
                .is_empty()
        );
        assert!(owned_loops(&loops.replace("loop7", "loop7junk"), image).is_err());
        let row = json!({"target":point,"source":"/dev/loop7","fstype":"ext4"});
        exact_mount(
            &json!({"filesystems":[row]}).to_string(),
            point,
            "/dev/loop7",
        )
        .unwrap();
        for (k, v) in [
            ("target", json!("/")),
            ("source", json!("/dev/root")),
            ("fstype", json!("tmpfs")),
        ] {
            assert!(
                exact_mount(
                    &json!({"filesystems":[mutate(&row,k,v)]}).to_string(),
                    point,
                    "/dev/loop7"
                )
                .is_err()
            );
        }
    }
    #[test]
    fn real_quota_requires_kernel_errno_global_space_and_no_privileges() {
        let good = probe();
        verify_probe(&good, UID, "fill", CHUNK, Space::Quota).unwrap();
        for (k, v) in [
            ("errno", json!(28)),
            ("errno", Value::Null),
            ("global_free_bytes", json!(1)),
            ("available_bytes", json!(1)),
            ("written", json!(9 * MIB)),
            ("pid", json!(true)),
        ] {
            assert!(
                verify_probe(&mutate(&good, k, v), UID, "fill", CHUNK, Space::Quota).is_err(),
                "{k}"
            );
        }
        let mut bad = good;
        bad["identity"]["caps"]["CapEff"] = json!(1);
        assert!(verify_probe(&bad, UID, "fill", CHUNK, Space::Quota).is_err());
    }
    #[test]
    fn full_current_and_retained_oracles_reject_damage_and_stale_state() {
        for rev in [3, 4] {
            let good = fixture_expected(SOURCE, "inspect", rev, 42);
            fixture(&good, SOURCE, "inspect", rev, 42).unwrap();
            for (k, v) in [
                ("source", json!("wrong")),
                ("features", json!(1)),
                ("current_revision", json!(7 - rev)),
                ("retained_revision", json!(3)),
                ("pid", json!(true)),
                ("pid", json!(43)),
            ] {
                assert!(
                    fixture(&mutate(&good, k, v), SOURCE, "inspect", rev, 42).is_err(),
                    "{k}"
                );
            }
            for field in ["current_rows", "retained_rows"] {
                let mut v = good.clone();
                let orig = v[field][0]["bytes_hex"].as_str().unwrap();
                v[field][0]["bytes_hex"] = json!(format!("00{}", &orig[2..]));
                assert!(fixture(&v, SOURCE, "inspect", rev, 42).is_err());
                let mut v = good.clone();
                v[field][1]["integer"] = json!(v[field][1]["integer"].as_u64().unwrap() as f64);
                assert!(fixture(&v, SOURCE, "inspect", rev, 42).is_err());
            }
        }
    }
    #[test]
    fn quota_publication_diagnostics_and_terminal_state_are_required() {
        let good = fixture_expected(SOURCE, "fail", 3, 42);
        fixture(&good, SOURCE, "fail", 3, 42).unwrap();
        for (k, v) in [
            ("commit_error", json!(0)),
            ("native_raw_code", json!(28)),
            ("terminal_error", json!(0)),
            ("recovery_required", json!(false)),
            ("blocked_write_error", json!(0)),
        ] {
            assert!(
                fixture(&mutate(&good, k, v), SOURCE, "fail", 3, 42).is_err(),
                "{k}"
            );
        }
    }
    fn failure(space: Space) -> Value {
        json!({"stage":"fail","operation":"backup","pid":12,"error":14,"category":space.category(),"raw":space.errno(),"native_operation":"Write","partial_bytes":8*MIB,"source_unchanged":true,"backup_unchanged":true})
    }
    #[test]
    fn maintenance_partial_copy_requires_classified_errno_and_unchanged_inputs() {
        for space in [Space::Quota, Space::Enospc] {
            let done = resource_oracle::completion("space-fail", SOURCE);
            let good = failure(space);
            maintenance(
                &[good.clone(), done.clone()],
                "space-fail",
                "backup",
                SOURCE,
                space,
                12,
            )
            .unwrap();
            for (k, v) in [
                ("error", json!(15)),
                ("raw", json!(if space == Space::Quota { 28 } else { 122 })),
                ("native_operation", json!("Create")),
                ("partial_bytes", json!(0)),
                ("partial_bytes", json!(63_074_304)),
                ("source_unchanged", json!(false)),
                ("backup_unchanged", json!(1)),
                ("pid", json!(true)),
                ("pid", json!(13)),
            ] {
                assert!(
                    maintenance(
                        &[mutate(&good, k, v), done.clone()],
                        "space-fail",
                        "backup",
                        SOURCE,
                        space,
                        12
                    )
                    .is_err(),
                    "{k}"
                );
            }
            for bad in [
                vec![],
                vec![good.clone()],
                vec![good.clone(), done.clone(), done.clone()],
                vec![good.clone(), mutate(&done, "source_id", json!("wrong"))],
                vec![good, mutate(&done, "features", json!(4.0))],
            ] {
                assert!(maintenance(&bad, "space-fail", "backup", SOURCE, space, 12).is_err());
            }
        }
    }
    #[test]
    fn maintenance_retry_and_reopen_require_right_disposition_and_bytes() {
        for op in ["backup", "restore", "compact"] {
            let row = json!({"stage":"retry","operation":op,"pid":12,"partial_rejected":true,"existing_rejected":op!="compact","partial_preserved":true,"matches_backup":true});
            let done = resource_oracle::completion("space-retry", SOURCE);
            maintenance(
                &[row.clone(), done.clone()],
                "space-retry",
                op,
                SOURCE,
                Space::Quota,
                12,
            )
            .unwrap();
            for k in [
                "partial_rejected",
                "existing_rejected",
                "partial_preserved",
                "matches_backup",
            ] {
                assert!(
                    maintenance(
                        &[
                            mutate(&row, k, json!(!row[k].as_bool().unwrap())),
                            done.clone()
                        ],
                        "space-retry",
                        op,
                        SOURCE,
                        Space::Quota,
                        12
                    )
                    .is_err()
                );
            }
        }
        let check = json!({"phase":"space_initial-source","revision":212,"seq":4,"changed":false,"roots":64,"digests":resource_oracle::digests(4,false),"live_bytes":resource_oracle::CORE/2,"peak_bytes":resource_oracle::CORE,"cap_bytes":resource_oracle::CORE,"rss_kib":1,"file_bytes":66_154_496,"elapsed_ns":0,"detail":{"fresh_process":true}});
        let done = resource_oracle::completion("space-check", SOURCE);
        maintenance(
            &[check.clone(), done.clone()],
            "space-check",
            "initial-source",
            SOURCE,
            Space::Enospc,
            12,
        )
        .unwrap();
        for (k, v) in [
            ("revision", json!(213)),
            ("roots", json!(63)),
            ("digests", json!([0, 0, 0, 0])),
        ] {
            assert!(
                maintenance(
                    &[mutate(&check, k, v), done.clone()],
                    "space-check",
                    "initial-source",
                    SOURCE,
                    Space::Enospc,
                    12
                )
                .is_err(),
                "{k}"
            );
        }
    }
    #[test]
    fn maintenance_ballast_checks_larger_ceiling_and_distinct_kernel_errors() {
        let mut good = probe();
        good["written"] = json!(128 * MIB);
        good["ceiling"] = json!(CEILING);
        verify_probe(&good, UID, "fill", CEILING, Space::Quota).unwrap();
        for (k, v) in [
            ("errno", json!(28)),
            ("written", json!(CEILING + 1)),
            ("available_bytes", json!(0)),
        ] {
            assert!(
                verify_probe(&mutate(&good, k, v), UID, "fill", CEILING, Space::Quota).is_err()
            );
        }
        good["errno"] = json!(28);
        good["global_free_bytes"] = json!(0);
        good["available_bytes"] = json!(0);
        verify_probe(&good, UID, "fill", CEILING, Space::Enospc).unwrap();
        use clap::Parser;
        assert!(
            crate::Cli::try_parse_from([
                "xtask",
                "quota",
                "--fixture",
                "unused",
                "--source",
                SOURCE,
                "--output",
                "unused",
                "--maintenance",
                "--probe-only"
            ])
            .is_err()
        );
    }
}
