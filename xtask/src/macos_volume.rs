//! Own and verify a bounded APFS disk image for the maintenance fixture.
use crate::{
    delivery,
    support::{Scratch, cmd, path},
    trace_process::{call, execute},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::{CString, OsString},
    fs, io,
    os::unix::ffi::OsStrExt,
    path::Path,
};

const HDIUTIL: &str = "/usr/bin/hdiutil";
const PLUTIL: &str = "/usr/bin/plutil";
const SIZE: u64 = 256 * 1024 * 1024;
const BLOCK: u64 = 4096;
const DETACH_ATTEMPTS: usize = 5;

fn retry_detach(
    mut attempt: impl FnMut(usize) -> Result<crate::trace_process::Report>,
    mut on_busy: impl FnMut(),
) -> Result<()> {
    for number in 0..DETACH_ATTEMPTS {
        let report = attempt(number)?;
        ensure!(
            report.qualifies(true),
            "detach supervision failed: {report:?}"
        );
        match report.exit {
            Some(0) => return Ok(()),
            Some(16) if number + 1 < DETACH_ATTEMPTS => on_busy(),
            _ => anyhow::bail!(
                "owned APFS detach failed on attempt {}: {report:?}",
                number + 1
            ),
        }
    }
    unreachable!("last detach attempt returns an outcome")
}

pub struct Volume {
    device: String,
    mount: String,
    filesystem: String,
    capacity: u64,
    available: u64,
    block: u64,
}

fn stat(path: &Path) -> Result<Volume> {
    let name = CString::new(path.as_os_str().as_bytes())?;
    let mut result = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: valid NUL-terminated path and writable statfs output.
    if unsafe { libc::statfs(name.as_ptr(), result.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error()).context("statfs for disk image");
    }
    // SAFETY: the successful call initialized the statistic fields.
    let result = unsafe { result.assume_init() };
    fn text(field: &[libc::c_char]) -> Result<String> {
        let bytes = field
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8)
            .collect();
        Ok(String::from_utf8(bytes)?)
    }
    let block = u64::from(result.f_bsize);
    Ok(Volume {
        device: text(&result.f_mntfromname)?,
        mount: text(&result.f_mntonname)?,
        filesystem: text(&result.f_fstypename)?,
        capacity: result
            .f_blocks
            .checked_mul(block)
            .context("APFS capacity overflow")?,
        available: result
            .f_bavail
            .checked_mul(block)
            .context("APFS available size overflow")?,
        block,
    })
}

impl Volume {
    fn evidence(&self) -> Value {
        json!({"device":self.device,"mount":self.mount,"filesystem":self.filesystem,
            "total_bytes":self.capacity,"free_before":self.available,"allocation_unit_bytes":self.block})
    }

    fn admit_private(&self) -> Result<()> {
        ensure!(
            self.filesystem == "apfs"
                && (SIZE / 2..=SIZE).contains(&self.capacity)
                && self.block == BLOCK
                && self.available <= self.capacity,
            "bounded private APFS volume required"
        );
        Ok(())
    }
}

pub fn host_volume(output: &Path) -> Result<Value> {
    let volume = stat(output)?;
    ensure!(
        volume.filesystem == "apfs" && volume.available >= 2 * SIZE,
        "APFS backing volume with headroom required"
    );
    Ok(volume.evidence())
}

pub fn space(directory: &Path) -> Result<u64> {
    let volume = stat(directory)?;
    volume.admit_private()?;
    Ok(volume.available)
}

fn whole_disk(value: &str) -> bool {
    value
        .strip_prefix("/dev/disk")
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
}

fn owned_disk(info: &Value, image: &str) -> Result<Option<Value>> {
    let images = info["images"].as_array().context("disk image inventory")?;
    let mut owned = images.iter().filter(|item| item["image-path"] == image);
    let result = owned.next().cloned();
    ensure!(
        owned.next().is_none(),
        "duplicate attachment for owned image"
    );
    Ok(result)
}

fn query(image: &Path, temp: &Path, env: &BTreeMap<OsString, OsString>) -> Result<Option<Value>> {
    // The global inventory can contain unrelated mounts. Keep it in an owned
    // temporary directory and publish only the exact image's matched record.
    let scratch = Scratch::new(temp, "inventory")?;
    let mut commands = vec![];
    execute(
        cmd([HDIUTIL, "info", "-plist"]),
        scratch.path(),
        "images",
        env,
        &mut commands,
        false,
    )?;
    let mut args = cmd([PLUTIL, "-convert", "json", "-o", "-"]);
    args.push(path(scratch.path().join("images.stdout")));
    execute(args, scratch.path(), "parse", env, &mut commands, false)?;
    let list = delivery::read_json(&scratch.path().join("parse.stdout"))?;
    owned_disk(&list, image.to_str().context("UTF-8 image path")?)
}

fn disk(entry: &Value) -> Result<&str> {
    let mut disks = entry["system-entities"]
        .as_array()
        .context("owned disk entities")?
        .iter()
        .filter(|part| part["content-hint"] == "GUID_partition_scheme");
    let name = disks.next().context("owned GPT disk missing")?["dev-entry"]
        .as_str()
        .context("disk device node")?;
    ensure!(
        whole_disk(name) && disks.next().is_none(),
        "exact whole disk required"
    );
    Ok(name)
}

fn attach(
    proof: &mut Value,
    output: &Path,
    env: &BTreeMap<OsString, OsString>,
    label: &str,
) -> Result<Value> {
    let scratch = output.join("volume");
    let mount = scratch.join("mount");
    let image = scratch.join("owned.dmg");
    let mut args = cmd([HDIUTIL, "attach", "-plist", "-nobrowse", "-mountpoint"]);
    args.extend([path(&mount), path(&image)]);
    call(proof, output, args, &scratch, label, env, false)?;
    let attached = query(&image, &scratch, env)?.context("owned image absent from inventory")?;
    let physical = disk(&attached)?;
    ensure!(
        attached["blockcount"]
            .as_u64()
            .zip(attached["blocksize"].as_u64())
            .and_then(|(count, size)| count.checked_mul(size))
            == Some(SIZE),
        "wrong backing image geometry"
    );
    let volume = stat(&mount)?;
    volume.admit_private()?;
    let recorded = attached["system-entities"].as_array().unwrap();
    ensure!(
        volume.mount == mount.to_str().context("UTF-8 APFS mount")?
            && volume.device != stat(&scratch)?.device
            && recorded
                .iter()
                .any(|p| p["mount-point"] == volume.mount && p["dev-entry"] == volume.device),
        "APFS mount is not part of the owned image"
    );
    let mut evidence = volume.evidence();
    evidence["whole_disk"] = json!(physical);
    evidence["virtual_bytes"] = json!(SIZE);
    Ok(evidence)
}

fn detach_owned(
    proof: &mut Value,
    output: &Path,
    env: &BTreeMap<OsString, OsString>,
    label: &str,
    mut on_busy: impl FnMut(),
) -> Result<()> {
    let scratch = output.join("volume");
    let image = scratch.join("owned.dmg");
    let attached = query(&image, &scratch, env)?.context("owned image missing before detach")?;
    let expected = disk(&attached)?.to_owned();
    retry_detach(
        |number| {
            let current =
                query(&image, &scratch, env)?.context("owned image disappeared during detach")?;
            ensure!(
                disk(&current)? == expected,
                "owned image device changed during detach"
            );
            call(
                proof,
                output,
                cmd([HDIUTIL, "detach", &expected]),
                &scratch,
                &format!("{label}-attempt-{}", number + 1),
                env,
                true,
            )
        },
        || {
            on_busy();
            std::thread::sleep(std::time::Duration::from_millis(250));
        },
    )
}

pub fn remount(
    proof: &mut Value,
    output: &Path,
    env: &BTreeMap<OsString, OsString>,
) -> Result<Value> {
    let scratch = output.join("volume");
    let image = scratch.join("owned.dmg");
    detach_owned(proof, output, env, "detach-for-reopen", || {})?;
    ensure!(
        query(&image, &scratch, env)?.is_none()
            && stat(&scratch.join("mount"))?.device == stat(&scratch)?.device,
        "initial APFS mount did not retire"
    );
    attach(proof, output, env, "attach-for-reopen")
}

pub fn with_volume(
    proof: &mut Value,
    output: &Path,
    env: &BTreeMap<OsString, OsString>,
    test: impl FnOnce(&mut Value, &Path) -> Result<()>,
) -> Result<()> {
    let scratch = output.join("volume");
    fs::create_dir(&scratch)?;
    let mount = scratch.join("mount");
    fs::create_dir(&mount)?;
    let host = stat(&mount)?;
    let image = scratch.join("owned.dmg");
    let mut attempted_creation = false;
    let result = (|| {
        ensure!(
            query(&image, &scratch, env)?.is_none(),
            "owned image path already attached"
        );
        attempted_creation = true;
        let mut args = cmd([
            HDIUTIL,
            "create",
            "-size",
            "256m",
            "-fs",
            "APFS",
            "-layout",
            "GPTSPUD",
            "-volname",
            "WDB-ENOSPC",
            "-type",
            "UDIF",
            "-nospotlight",
        ]);
        args.push(path(&image));
        call(proof, output, args, &scratch, "create", env, false)?;
        proof["private_volume"] = attach(proof, output, env, "attach")?;
        let data = mount.join("data");
        fs::create_dir(&data)?;
        test(proof, &data)
    })();

    let cleanup = (|| -> Result<()> {
        ensure!(
            attempted_creation,
            "image setup did not establish ownership"
        );
        if query(&image, &scratch, env)?.is_some() {
            detach_owned(proof, output, env, "detach", || {})?;
        }
        ensure!(
            query(&image, &scratch, env)?.is_none() && stat(&mount)?.device == host.device,
            "owned APFS image remains attached"
        );
        match fs::remove_file(&image) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        ensure!(!image.try_exists()?, "owned disk image was not removed");
        Ok(())
    })();
    proof["volume_cleanup"] =
        json!({"verified":cleanup.is_ok(),"error":cleanup.as_ref().err().map(|e|format!("{e:#}"))});
    result?;
    cleanup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detach_retries_only_clean_busy_and_requires_a_clean_success() {
        for (exits, success, waits) in [
            (vec![0], true, 0),
            (vec![16, 0], true, 1),
            (vec![16; DETACH_ATTEMPTS], false, DETACH_ATTEMPTS - 1),
            (vec![1], false, 0),
        ] {
            let mut calls = 0;
            let mut pauses = 0;
            let result = retry_detach(
                |number| {
                    calls += 1;
                    Ok(crate::trace_process::Report {
                        exit: Some(exits[number]),
                        cleanup_verified: true,
                        ..Default::default()
                    })
                },
                || pauses += 1,
            );
            assert_eq!(result.is_ok(), success);
            assert_eq!(calls, exits.len());
            assert_eq!(pauses, waits);
        }
        for (cleanup, failure, exit) in [
            (false, None, Some(0)),
            (true, Some("timeout"), Some(16)),
            (true, None, None),
        ] {
            let mut calls = 0;
            let result = retry_detach(
                |_| {
                    calls += 1;
                    Ok(crate::trace_process::Report {
                        exit,
                        cleanup_verified: cleanup,
                        failure: failure.map(str::to_owned),
                        ..Default::default()
                    })
                },
                || panic!("failed supervision cannot retry"),
            );
            assert!(result.is_err());
            assert_eq!(calls, 1);
        }
    }

    #[test]
    #[ignore = "creates a private native APFS image and holds its directory open"]
    fn native_busy_detach_releases_only_the_owned_image() {
        let mut root =
            crate::support::Scratch::new(&std::env::temp_dir(), "apfs-detach-control").unwrap();
        let output = root.path().join("proof");
        fs::create_dir(&output).unwrap();
        let run = crate::support::Run::new(root.path().to_owned());
        let mut proof = json!({"commands":[]});
        let result = with_volume(&mut proof, &output, &run.env, |proof, data| {
            let mut held = Some(fs::File::open(data)?);
            let result = detach_owned(proof, &output, &run.env, "controlled-detach", || {
                drop(held.take());
            });
            drop(held.take());
            result?;
            let exits = proof["commands"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["command"][1] == "detach")
                .map(|row| row["report"]["exit"].as_i64().unwrap())
                .collect::<Vec<_>>();
            ensure!(
                exits.first() == Some(&16) && exits.last() == Some(&0),
                "control must observe Busy before successful detach"
            );
            Ok(())
        });
        assert_eq!(proof["volume_cleanup"]["verified"], true);
        result.unwrap();
        root.close().unwrap();
    }

    #[test]
    fn cleanup_admits_only_exact_image_and_whole_gpt_device() {
        let owned = json!({"image-path":"/scratch/owned.dmg","system-entities":[
            {"content-hint":"GUID_partition_scheme","dev-entry":"/dev/disk6"},
            {"content-hint":"Apple_APFS","dev-entry":"/dev/disk6s1"}
        ]});
        let system = json!({"images":[{"image-path":"/other/owned.dmg"},owned]});
        assert!(owned_disk(&system, "/scratch/a.dmg").unwrap().is_none());
        assert_eq!(
            disk(&owned_disk(&system, "/scratch/owned.dmg").unwrap().unwrap()).unwrap(),
            "/dev/disk6"
        );
        assert!(owned_disk(&json!({"images":[owned,owned]}), "/scratch/owned.dmg").is_err());
        assert!(!whole_disk("/dev/disk6s1"));
    }
}
