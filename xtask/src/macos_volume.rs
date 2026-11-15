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
        let mut args = cmd([HDIUTIL, "attach", "-plist", "-nobrowse", "-mountpoint"]);
        args.extend([path(&mount), path(&image)]);
        call(proof, output, args, &scratch, "attach", env, false)?;
        let attached =
            query(&image, &scratch, env)?.context("owned image absent from inventory")?;
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
                && volume.device != host.device
                && recorded
                    .iter()
                    .any(|p| p["mount-point"] == volume.mount && p["dev-entry"] == volume.device),
            "APFS mount is not part of the owned image"
        );
        let mut evidence = volume.evidence();
        evidence["whole_disk"] = json!(physical);
        evidence["virtual_bytes"] = json!(SIZE);
        proof["private_volume"] = evidence;
        let data = mount.join("data");
        fs::create_dir(&data)?;
        test(proof, &data)
    })();

    let cleanup = (|| -> Result<()> {
        ensure!(
            attempted_creation,
            "image setup did not establish ownership"
        );
        if let Some(attached) = query(&image, &scratch, env)? {
            call(
                proof,
                output,
                cmd([HDIUTIL, "detach", disk(&attached)?]),
                &scratch,
                "detach",
                env,
                false,
            )?;
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
