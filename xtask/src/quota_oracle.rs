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
pub const EXT4_INODES: u64 = 128;
pub const MAX_CALIBRATION_INODES: u64 = 2 * EXT4_INODES;
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum CopyBarrier {
    Data,
    Slot,
    Witness,
    FileMetadata,
    ExchangeDirectory,
    ReplacementFile,
    CleanupDirectory,
    CleanupFile,
}

impl CopyBarrier {
    pub fn name(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Slot => "slot",
            Self::Witness => "witness",
            Self::FileMetadata => "file-metadata",
            Self::ExchangeDirectory => "exchange-directory",
            Self::ReplacementFile => "replacement-file",
            Self::CleanupDirectory => "cleanup-directory",
            Self::CleanupFile => "cleanup-file",
        }
    }

    pub fn offset(self) -> Option<u64> {
        // The near-cap copy publishes even revision 212.
        match self {
            Self::Data => Some(0),
            Self::Slot => Some(4096),
            Self::Witness => Some(12288),
            Self::FileMetadata
            | Self::ExchangeDirectory
            | Self::ReplacementFile
            | Self::CleanupDirectory
            | Self::CleanupFile => None,
        }
    }

    pub fn fault_journal(self) -> bool {
        self.offset().is_none()
    }

    pub fn published_file_sync(self) -> bool {
        matches!(self, Self::ReplacementFile | Self::CleanupFile)
    }

    pub fn live_scratch(self) -> Option<&'static str> {
        match self {
            Self::ExchangeDirectory | Self::ReplacementFile => Some("original"),
            Self::CleanupDirectory | Self::CleanupFile => Some("absent"),
            _ => None,
        }
    }

    pub fn remount_keeps_replacement(self) -> bool {
        self.published_file_sync() || self == Self::CleanupDirectory
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Quota,
    Enospc,
    InodeQuota,
    InodeEnospc,
    Writeback(CopyBarrier),
}
impl Space {
    pub fn errno(self) -> i32 {
        match self {
            Self::Enospc | Self::InodeEnospc => 28,
            Self::Quota | Self::InodeQuota => 122,
            Self::Writeback(_) => 5,
        }
    }
    pub fn fails_at_create(self) -> bool {
        matches!(self, Self::InodeQuota | Self::InodeEnospc)
    }
    pub fn profile(self) -> &'static str {
        match self {
            Self::Quota => "quota",
            Self::Enospc => "enospc",
            Self::InodeQuota => "inode-quota",
            Self::InodeEnospc => "inode-enospc",
            Self::Writeback(CopyBarrier::Data) => "writeback",
            Self::Writeback(CopyBarrier::Slot) => "writeback-slot",
            Self::Writeback(CopyBarrier::Witness) => "writeback-witness",
            Self::Writeback(CopyBarrier::FileMetadata) => "writeback-file-metadata",
            Self::Writeback(CopyBarrier::ExchangeDirectory) => "writeback-exchange-directory",
            Self::Writeback(CopyBarrier::ReplacementFile) => "writeback-replacement-file",
            Self::Writeback(CopyBarrier::CleanupDirectory) => "writeback-cleanup-directory",
            Self::Writeback(CopyBarrier::CleanupFile) => "writeback-cleanup-file",
        }
    }
    pub fn fault_journal(self) -> bool {
        matches!(self, Self::Writeback(barrier) if barrier.fault_journal())
    }
    pub fn category(self) -> &'static str {
        match self {
            Self::Enospc | Self::InodeEnospc => "DiskFull",
            Self::Quota | Self::InodeQuota => "Quota",
            Self::Writeback(_) => "Other",
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
            space == Space::Enospc || bytes > 16 * MIB,
            "global data headroom too low to isolate the limit"
        );
    }
    match mode {
        "fill" => ensure!(
            row["errno"] == space.errno()
                && row["ceiling"] == ceiling
                && number(row, "written")? <= ceiling,
            "actual bounded expected kernel space error required"
        ),
        "inode-fill" => ensure!(
            space == Space::InodeQuota
                && row["errno"] == 122
                && row["fsynced_bytes"] == 4096
                && number(row, "global_free_inodes")? > 16,
            "actual inode quota with global inode and data headroom required"
        ),
        "inode-exhaust" => ensure!(
            space == Space::InodeEnospc
                && row["errno"] == space.errno()
                && row["fsynced_bytes"] == 4096
                && number(row, "global_free_inodes")? == 0
                && (1..=MAX_CALIBRATION_INODES).contains(&number(row, "created")?),
            "bounded global inode exhaustion with data headroom required"
        ),
        "fill-inode-ballast" => ensure!(
            space == Space::Enospc
                && row["available_bytes"] == 0
                && row["global_free_inodes"] == 0
                && row["errno"] == 28
                && row["create_errno"] == 28
                && row["ceiling"] == ceiling
                && (16 * MIB..=ceiling).contains(&number(row, "written")?),
            "publication requires real block and inode ENOSPC with no user headroom"
        ),
        "inode-release" => ensure!(
            space == Space::InodeEnospc
                && row["removed"] == true
                && row["before_free_inodes"] == 0
                && row["global_free_inodes"] == 1,
            "exactly one global inode must be freed after the reopens"
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
pub fn fixture(row: &Value, source: &str, stage: &str, revision: u64, pid: u32) -> Result<()> {
    crate::quota_append::verify(row, source, stage, revision, pid, 122)
}
#[cfg(test)]
fn fixture_expected(source: &str, stage: &str, revision: u64, pid: u32) -> Value {
    crate::quota_append::expected(source, stage, revision, pid, 122)
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
    ensure!(
        !space.fault_journal() || label == "compact",
        "journal barrier requires in-place compaction"
    );
    let expected = match mode {
        "space-publish" if space == Space::InodeEnospc && label == "compact" => {
            json!({"stage":"publish","operation":label,"pid":pid,"before_bytes":66_154_496,"after_bytes":63_074_304,"matches_backup":true,"scratch_removed":true})
        }
        "space-fail" if space.fails_at_create() => {
            json!({"stage":"fail","operation":label,"pid":pid,"error":14,"category":space.category(),"raw":space.errno(),"native_operation":"Create","partial_bytes":0,"source_unchanged":true,"backup_unchanged":true,"inventory_unchanged":true})
        }
        "space-fail" if matches!(space, Space::Writeback(barrier) if barrier.live_scratch().is_some()) =>
        {
            let Space::Writeback(barrier) = space else {
                unreachable!()
            };
            let scratch = barrier.live_scratch().unwrap();
            json!({"stage":"fail","operation":label,"pid":pid,"error":15,"category":"Other","raw":5,"native_operation":"Sync","source_matches_backup":true,"scratch":scratch,"backup_unchanged":true})
        }
        "space-fail" if matches!(space, Space::Writeback(_)) => {
            let error = if space == Space::Writeback(CopyBarrier::Data) || label == "compact" {
                14
            } else {
                15
            };
            json!({"stage":"fail","operation":label,"pid":pid,"error":error,"category":"Other","raw":5,"native_operation":"Sync","partial_bytes":63_074_304,"source_unchanged":true,"backup_unchanged":true})
        }
        "space-fail" => {
            let partial = number(row, "partial_bytes")?;
            ensure!(
                (MIB..63_074_304).contains(&partial)
                    && matches!(row["native_operation"].as_str(), Some("Write" | "Sync")),
                "not a partial-copy exhaustion"
            );
            json!({"stage":"fail","operation":label,"pid":pid,"error":14,"category":space.category(),"raw":space.errno(),"native_operation":row["native_operation"],"partial_bytes":partial,"source_unchanged":true,"backup_unchanged":true})
        }
        "space-retry"
            if space.fails_at_create() || space == Space::Writeback(CopyBarrier::CleanupFile) =>
        {
            json!({"stage":"retry","operation":label,"pid":pid,"no_partial":true,"matches_backup":true})
        }
        "space-retry" if space.fault_journal() => {
            json!({"stage":"retry","operation":label,"pid":pid,"partial_verified_witnessed":true,"existing_rejected":false,"partial_preserved":true,"matches_backup":true})
        }
        "space-retry" if space == Space::Writeback(CopyBarrier::Witness) => {
            json!({"stage":"retry","operation":label,"pid":pid,"partial_verified_unwitnessed":true,"existing_rejected":label!="compact","partial_preserved":true,"matches_backup":true})
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
pub fn formatter(executable: &Path, image: &Path, space: Space) -> Vec<OsString> {
    let mut args = vec![path(executable)];
    args.extend(cmd(["-t", "ext4", "-q", "-F", "-O"]));
    if space == Space::InodeEnospc {
        args.extend(cmd(["^quota", "-N", &EXT4_INODES.to_string()]));
    } else {
        args.extend(cmd(["quota", "-E", "quotatype=usrquota"]));
    }
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
                Path::new("/owned/image.ext4"),
                Space::Quota
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
        assert_eq!(
            formatter(
                Path::new("/usr/sbin/mke2fs"),
                Path::new("/owned/image.ext4"),
                Space::InodeEnospc
            ),
            cmd([
                "/usr/sbin/mke2fs",
                "-t",
                "ext4",
                "-q",
                "-F",
                "-O",
                "^quota",
                "-N",
                "128",
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
    fn global_inode_exhaustion_requires_zero_inodes_data_headroom_and_enospc() {
        let space = Space::InodeEnospc;
        let full = json!({"mode":"inode-exhaust","pid":42,"identity":identity(UID),"global_free_bytes":32*MIB,"available_bytes":32*MIB,"errno":28,"global_free_inodes":0,"created":1,"fsynced_bytes":4096});
        verify_probe(&full, UID, "inode-exhaust", CEILING, space).unwrap();
        for (key, value) in [
            ("global_free_inodes", json!(1)),
            ("available_bytes", json!(0)),
            ("errno", json!(122)),
            ("created", json!(MAX_CALIBRATION_INODES + 1)),
        ] {
            assert!(
                verify_probe(
                    &mutate(&full, key, value),
                    UID,
                    "inode-exhaust",
                    CEILING,
                    space
                )
                .is_err()
            );
        }
        let released = json!({"mode":"inode-release","pid":43,"identity":identity(UID),"global_free_bytes":32*MIB,"available_bytes":32*MIB,"before_free_inodes":0,"global_free_inodes":1,"removed":true});
        verify_probe(&released, UID, "inode-release", CEILING, space).unwrap();
        assert!(
            verify_probe(
                &mutate(&released, "global_free_inodes", json!(2)),
                UID,
                "inode-release",
                CEILING,
                space
            )
            .is_err()
        );
        let failure = json!({"stage":"fail","operation":"backup","pid":44,"error":14,"category":"DiskFull","raw":28,"native_operation":"Create","partial_bytes":0,"source_unchanged":true,"backup_unchanged":true,"inventory_unchanged":true});
        let completion = resource_oracle::completion("space-fail", SOURCE);
        maintenance(
            &[failure.clone(), completion.clone()],
            "space-fail",
            "backup",
            SOURCE,
            space,
            44,
        )
        .unwrap();
        for (key, value) in [
            ("native_operation", json!("Write")),
            ("raw", json!(122)),
            ("inventory_unchanged", json!(false)),
        ] {
            assert!(
                maintenance(
                    &[mutate(&failure, key, value), completion.clone()],
                    "space-fail",
                    "backup",
                    SOURCE,
                    space,
                    44
                )
                .is_err()
            );
        }
    }
    #[test]
    fn inode_quota_needs_a_create_error_with_no_partial_and_independent_headroom() {
        let space = Space::InodeQuota;
        let full = json!({"mode":"inode-fill","pid":42,"identity":identity(UID),"global_free_bytes":32*MIB,"available_bytes":32*MIB,"errno":122,"global_free_inodes":256,"fsynced_bytes":4096});
        verify_probe(&full, UID, "inode-fill", CEILING, space).unwrap();
        for (key, value) in [
            ("errno", json!(28)),
            ("global_free_inodes", json!(0)),
            ("fsynced_bytes", json!(0)),
        ] {
            assert!(
                verify_probe(
                    &mutate(&full, key, value),
                    UID,
                    "inode-fill",
                    CEILING,
                    space
                )
                .is_err()
            );
        }
        let mut fail = failure(space);
        fail["native_operation"] = json!("Create");
        fail["partial_bytes"] = json!(0);
        fail["inventory_unchanged"] = json!(true);
        let check = |row| {
            maintenance(
                &[row, resource_oracle::completion("space-fail", SOURCE)],
                "space-fail",
                "backup",
                SOURCE,
                space,
                12,
            )
            .is_ok()
        };
        assert!(check(fail.clone()));
        assert!(!check(mutate(&fail, "native_operation", json!("Write"))));
        assert!(!check(mutate(&fail, "partial_bytes", json!(4096))));
        assert!(!check(mutate(&fail, "inventory_unchanged", json!(false))));
    }
    #[test]
    fn delayed_maintenance_writeback_requires_full_length_incomplete_target_and_sync_eio() {
        let failure = json!({"stage":"fail","operation":"compact","pid":42,"error":14,"category":"Other","raw":5,"native_operation":"Sync","partial_bytes":63_074_304,"source_unchanged":true,"backup_unchanged":true});
        let complete = resource_oracle::completion("space-fail", SOURCE);
        maintenance(
            &[failure.clone(), complete.clone()],
            "space-fail",
            "compact",
            SOURCE,
            Space::Writeback(CopyBarrier::Data),
            42,
        )
        .unwrap();
        for (key, value) in [
            ("native_operation", json!("Write")),
            ("raw", json!(28)),
            ("partial_bytes", json!(63_070_208)),
            ("source_unchanged", json!(false)),
        ] {
            assert!(
                maintenance(
                    &[mutate(&failure, key, value), complete.clone()],
                    "space-fail",
                    "compact",
                    SOURCE,
                    Space::Writeback(CopyBarrier::Data),
                    42
                )
                .is_err()
            );
        }
    }

    #[test]
    fn maintenance_metadata_eio_keeps_copy_uncertainty_and_unwitnessed_inspection() {
        for barrier in [CopyBarrier::Slot, CopyBarrier::Witness] {
            for operation in ["backup", "restore", "compact"] {
                let error = if operation == "compact" { 14 } else { 15 };
                let mut row = json!({"stage":"fail","operation":operation,"pid":42,"error":error,"category":"Other","raw":5,"native_operation":"Sync","partial_bytes":63_074_304,"source_unchanged":true,"backup_unchanged":true});
                let space = Space::Writeback(barrier);
                let completion = resource_oracle::completion("space-fail", SOURCE);
                maintenance(
                    &[row.clone(), completion.clone()],
                    "space-fail",
                    operation,
                    SOURCE,
                    space,
                    42,
                )
                .unwrap();
                row["error"] = json!(29 - error);
                assert!(
                    maintenance(
                        &[row, completion],
                        "space-fail",
                        operation,
                        SOURCE,
                        space,
                        42
                    )
                    .is_err()
                );
            }
        }
        let retry = json!({"stage":"retry","operation":"backup","pid":42,"partial_verified_unwitnessed":true,"existing_rejected":true,"partial_preserved":true,"matches_backup":true});
        let completion = resource_oracle::completion("space-retry", SOURCE);
        maintenance(
            &[retry.clone(), completion.clone()],
            "space-retry",
            "backup",
            SOURCE,
            Space::Writeback(CopyBarrier::Witness),
            42,
        )
        .unwrap();
        assert!(
            maintenance(
                &[retry, completion],
                "space-retry",
                "backup",
                SOURCE,
                Space::Writeback(CopyBarrier::Slot),
                42
            )
            .is_err()
        );
    }

    #[test]
    fn file_metadata_writeback_requires_a_witnessed_compaction_scratch() {
        let retry = json!({"stage":"retry","operation":"compact","pid":42,"partial_verified_witnessed":true,"existing_rejected":false,"partial_preserved":true,"matches_backup":true});
        let completion = resource_oracle::completion("space-retry", SOURCE);
        let space = Space::Writeback(CopyBarrier::FileMetadata);
        maintenance(
            &[retry.clone(), completion.clone()],
            "space-retry",
            "compact",
            SOURCE,
            space,
            42,
        )
        .unwrap();
        assert!(
            maintenance(
                &[
                    mutate(&retry, "partial_verified_witnessed", json!(false)),
                    completion.clone()
                ],
                "space-retry",
                "compact",
                SOURCE,
                space,
                42
            )
            .is_err()
        );
        assert!(
            maintenance(
                &[mutate(&retry, "operation", json!("backup")), completion],
                "space-retry",
                "backup",
                SOURCE,
                space,
                42
            )
            .is_err()
        );
    }

    #[test]
    fn namespace_exhaustion_requires_both_kernel_errors_and_zero_user_headroom() {
        let full = json!({"mode":"fill-inode-ballast","pid":42,"identity":identity(UID),"global_free_bytes":8*MIB,"available_bytes":0,"global_free_inodes":0,"errno":28,"create_errno":28,"written":128*MIB,"ceiling":CEILING});
        let verify =
            |row: &Value| verify_probe(row, UID, "fill-inode-ballast", CEILING, Space::Enospc);
        verify(&full).unwrap();
        for (key, value) in [
            ("available_bytes", json!(4096)),
            ("global_free_inodes", json!(1)),
            ("errno", json!(122)),
            ("create_errno", json!(null)),
        ] {
            assert!(verify(&mutate(&full, key, value)).is_err(), "{key}");
        }
        let row = json!({"stage":"publish","operation":"compact","pid":42,"before_bytes":66_154_496,"after_bytes":63_074_304,"matches_backup":true,"scratch_removed":true});
        let done = resource_oracle::completion("space-publish", SOURCE);
        let publish = |row: Value| {
            maintenance(
                &[row, done.clone()],
                "space-publish",
                "compact",
                SOURCE,
                Space::InodeEnospc,
                42,
            )
            .is_ok()
        };
        assert!(publish(row.clone()));
        assert!(!publish(mutate(&row, "scratch_removed", json!(false))));
    }

    #[test]
    fn post_exchange_writeback_requires_uncertainty_and_the_matching_live_names() {
        for (barrier, scratch) in [
            (CopyBarrier::ExchangeDirectory, "original"),
            (CopyBarrier::ReplacementFile, "original"),
            (CopyBarrier::CleanupDirectory, "absent"),
            (CopyBarrier::CleanupFile, "absent"),
        ] {
            let space = Space::Writeback(barrier);
            let done = resource_oracle::completion("space-fail", SOURCE);
            let failure = json!({"stage":"fail","operation":"compact","pid":42,"error":15,"category":"Other","raw":5,"native_operation":"Sync","source_matches_backup":true,"scratch":scratch,"backup_unchanged":true});
            let verify = |row: Value| {
                maintenance(
                    &[row, done.clone()],
                    "space-fail",
                    "compact",
                    SOURCE,
                    space,
                    42,
                )
                .is_ok()
            };
            assert!(verify(failure.clone()));
            for (key, value) in [
                ("error", json!(14)),
                ("native_operation", json!("Write")),
                ("raw", json!(28)),
                ("source_matches_backup", json!(false)),
                (
                    "scratch",
                    json!(if scratch == "original" {
                        "absent"
                    } else {
                        "original"
                    }),
                ),
            ] {
                assert!(!verify(mutate(&failure, key, value)), "{key}");
            }
        }
    }

    #[test]
    fn post_exchange_retry_distinguishes_a_preserved_scratch_from_durable_unlink() {
        let done = resource_oracle::completion("space-retry", SOURCE);
        let absent = json!({"stage":"retry","operation":"compact","pid":42,"no_partial":true,"matches_backup":true});
        let present = json!({"stage":"retry","operation":"compact","pid":42,"partial_verified_witnessed":true,"existing_rejected":false,"partial_preserved":true,"matches_backup":true});
        for (barrier, good, wrong) in [
            (CopyBarrier::ReplacementFile, &present, &absent),
            (CopyBarrier::CleanupFile, &absent, &present),
        ] {
            let verify = |row: &Value| {
                maintenance(
                    &[row.clone(), done.clone()],
                    "space-retry",
                    "compact",
                    SOURCE,
                    Space::Writeback(barrier),
                    42,
                )
                .is_ok()
            };
            assert!(verify(good));
            assert!(!verify(wrong));
        }
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
