//! Qualify near-cap copies on an owned NTFS volume using the compound fixture.

use crate::resource_oracle as oracle;
use anyhow::{Result, ensure};
use serde_json::{Value, json};

#[cfg(windows)]
const SOURCE_BYTES: u64 = 66_154_496;
const COPY_BYTES: u64 = 63_074_304;
const OPERATIONS: [&str; 3] = ["backup", "restore", "compact"];

fn verify_phase(rows: &[Value], source: &str, mode: &str, label: &str, pid: u32) -> Result<()> {
    ensure!(
        rows.len() == 2 && rows[1] == oracle::completion(mode, source),
        "incomplete/wrong fixture identity"
    );
    let row = &rows[0];
    if matches!(mode, "space-check" | "space-advance") {
        let advanced = matches!(label, "advanced" | "advanced-source");
        oracle::state(
            row,
            &format!("space_{label}"),
            212 + u64::from(advanced),
            advanced,
            None,
        )?;
        ensure!(
            row["detail"] == json!({"fresh_process":true}),
            "missing fresh process verification"
        );
        return Ok(());
    }
    ensure!(
        OPERATIONS.contains(&label) && pid != 0,
        "invalid maintenance phase"
    );
    let expected = match mode {
        "space-fail" => {
            let partial = oracle::number(row, "partial_bytes")?;
            ensure!(
                (oracle::MIB..COPY_BYTES).contains(&partial),
                "partial copy length"
            );
            json!({"stage":"fail","operation":label,"pid":pid,"error":14,"category":"DiskFull","raw":112,
                "native_operation":"Write","partial_bytes":partial,"source_unchanged":true,"backup_unchanged":true})
        }
        "space-retry" => {
            json!({"stage":"retry","operation":label,"pid":pid,"partial_rejected":true,
            "existing_rejected":true,"partial_preserved":true,"matches_backup":true})
        }
        "space-retire" => {
            json!({"stage":"retire","operation":label,"pid":pid,"owned_outputs_removed":true})
        }
        _ => anyhow::bail!("unknown NTFS maintenance phase"),
    };
    ensure!(
        *row == expected,
        "wrong NTFS {mode} observation for {label}"
    );
    Ok(())
}

#[cfg(windows)]
mod native {
    use super::*;
    use crate::{
        delivery,
        support::{Run, path, sha},
        windows_trace::{admission, call, write_proof},
        windows_volume::{self, CLUSTER, Capacity},
    };
    use anyhow::Context;
    use clap::Args;
    use std::{
        collections::{BTreeMap, BTreeSet},
        ffi::OsString,
        fs::{self, OpenOptions},
        io::{Seek, SeekFrom, Write},
        path::{Path, PathBuf},
    };

    const CAPACITY: Capacity = Capacity::Maintenance;
    const HEADROOM: u64 = 8 * oracle::MIB;

    #[derive(Args)]
    pub struct Options {
        #[arg(long)]
        fixture: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        output: PathBuf,
    }

    struct Fixture<'a> {
        binary: &'a Path,
        runs: &'a Path,
        output: &'a Path,
        source: &'a str,
        env: BTreeMap<OsString, OsString>,
        processes: BTreeSet<u32>,
    }

    impl Fixture<'_> {
        fn command(
            &mut self,
            proof: &mut Value,
            args: Vec<OsString>,
            label: &str,
            expect_panic: bool,
        ) -> Result<(PathBuf, u32)> {
            let mut command = vec![path(self.binary)];
            command.extend(args);
            let report = call(
                proof,
                self.output,
                command,
                self.runs,
                label,
                &self.env,
                expect_panic,
            )?;
            let pid = report.pid.context("fixture process ID")?;
            ensure!(
                self.processes.insert(pid),
                "fixture process reused for {label}"
            );
            if expect_panic {
                let stderr = fs::read_to_string(self.runs.join(format!("{label}.stderr")))?;
                ensure!(
                    report.exit == Some(101)
                        && stderr.contains("Result::unwrap_err()")
                        && stderr.contains("CompactionStats"),
                    "negative control failed before the required assertion"
                );
            }
            Ok((self.runs.join(format!("{label}.stdout")), pid))
        }

        fn phase(
            &mut self,
            proof: &mut Value,
            data: &Path,
            image: &Path,
            mode: &str,
            label: &str,
        ) -> Result<Value> {
            let mut args = vec![path(data), path(image), mode.into(), label.into()];
            if label == "advanced-source" {
                args.push("changed".into());
            }
            let (stdout, pid) = self.command(proof, args, &format!("{mode}-{label}"), false)?;
            let rows = rows(&stdout)?;
            verify_phase(&rows, self.source, mode, label, pid)?;
            let result = json!({"mode":mode,"label":label,"pid":pid,"observation":rows[0]});
            proof["maintenance_records"]
                .as_array_mut()
                .unwrap()
                .push(result.clone());
            write_proof(self.output, proof)?;
            Ok(result)
        }
    }

    fn rows(path: &Path) -> Result<Vec<Value>> {
        fs::read_to_string(path)?
            .lines()
            .map(|line| delivery::json(line.as_bytes()))
            .collect()
    }

    fn bounded_fill(dir: &Path, ballast: &Path, new: bool) -> Result<u64> {
        let free = windows_volume::space(dir, CAPACITY)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(new)
            .open(ballast)?;
        let mut filled = file.seek(SeekFrom::End(0))?;
        // Bound both the expected capacity and the individual requests, including
        // the last rejected request. Only extend the controller's own ballast.
        let block = [0x5a; 65536];
        let mut request = block.len();
        loop {
            ensure!(
                filled + request as u64 <= 256 * oracle::MIB,
                "ballast write ceiling"
            );
            match file.write(&block[..request]) {
                Ok(n) if n > 0 => filled += n as u64,
                Err(e) if e.raw_os_error() == Some(112) => {
                    if request == CLUSTER as usize {
                        break;
                    }
                    request = CLUSTER as usize;
                }
                other => anyhow::bail!("unexpected ballast write: {other:?}"),
            }
        }
        file.sync_all()?;
        drop(file);
        ensure!(
            windows_volume::space(dir, CAPACITY)? == 0 && filled > free.saturating_sub(oracle::MIB),
            "actual full private NTFS required"
        );
        Ok(filled)
    }

    fn verified_copy(source: &Path, target: &Path, length: u64) -> Result<String> {
        ensure!(
            fs::metadata(source)?.len() == length,
            "wrong near-cap seed length"
        );
        let hash = sha(source)?;
        ensure!(fs::copy(source, target)? == length, "short seed install");
        OpenOptions::new().write(true).open(target)?.sync_all()?;
        ensure!(sha(target)? == hash, "copy hash mismatch");
        Ok(hash)
    }

    fn qualify(
        fixture: &mut Fixture<'_>,
        proof: &mut Value,
        seed: &Path,
        data: &Path,
    ) -> Result<()> {
        let source = data.join("source.wdb");
        let backup = data.join("backup.wdb");
        let source_hash = verified_copy(&seed.join("source.wdb"), &source, SOURCE_BYTES)?;
        let backup_hash = verified_copy(&seed.join("backup.wdb"), &backup, COPY_BYTES)?;
        proof["installed_seed"] = json!({"source_sha256":source_hash,"source_bytes":SOURCE_BYTES,
            "backup_sha256":backup_hash,"backup_bytes":COPY_BYTES});
        fixture.phase(proof, data, &source, "space-check", "initial-source")?;
        fixture.phase(proof, data, &backup, "space-check", "initial-backup")?;
        let (_, pid) = fixture.command(
            proof,
            vec![
                path(data),
                path(&source),
                "space-fail".into(),
                "backup".into(),
            ],
            "ample-space-control",
            true,
        )?;
        let control = data.join("failed-backup.wdb");
        ensure!(
            sha(&control)? == backup_hash,
            "negative control did not produce working backup bytes"
        );
        fixture.phase(proof, data, &control, "space-check", "ample-space-backup")?;
        fs::remove_file(control)?;
        proof["negative_control"] = json!({"pid":pid,"exit":101,"copy_verified":true});

        let ballast = data.join("ballast");
        for op in OPERATIONS {
            let filled = bounded_fill(data, &ballast, true)?;
            {
                let file = OpenOptions::new().write(true).open(&ballast)?;
                file.set_len(
                    filled
                        .checked_sub(HEADROOM)
                        .context("insufficient ballast")?,
                )?;
                file.sync_all()?;
            }
            ensure!(
                windows_volume::space(data, CAPACITY)? == HEADROOM,
                "exact 8 MiB of copy headroom required"
            );
            let failure = fixture.phase(proof, data, &source, "space-fail", op)?;
            ensure!(
                sha(&source)? == source_hash && sha(&backup)? == backup_hash,
                "failed operation modified a good image"
            );
            let partial = data.join(format!("failed-{op}.wdb"));
            let partial_hash = sha(&partial)?;
            let remaining = windows_volume::space(data, CAPACITY)?;
            ensure!(remaining < HEADROOM, "failed operation consumed no space");
            bounded_fill(data, &ballast, false)?;
            let full_source = fixture.phase(
                proof,
                data,
                &source,
                "space-check",
                &format!("full-source-{op}"),
            )?;
            let full_backup = fixture.phase(
                proof,
                data,
                &backup,
                "space-check",
                &format!("full-backup-{op}"),
            )?;
            ensure!(
                windows_volume::space(data, CAPACITY)? == 0,
                "fresh opens freed private space"
            );
            fs::remove_file(&ballast)?;
            let freed = windows_volume::space(data, CAPACITY)?;
            ensure!(
                freed >= COPY_BYTES,
                "ballast removal did not restore copy headroom"
            );
            let control = data.join("write-control");
            {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&control)?;
                file.write_all(&[0x6a; CLUSTER as usize])?;
                file.sync_all()?;
            }
            fs::remove_file(control)?;
            let retry = fixture.phase(proof, data, &source, "space-retry", op)?;
            let retried = data.join(format!("retry-{op}.wdb"));
            ensure!(
                sha(&retried)? == backup_hash
                    && sha(&backup)? == backup_hash
                    && sha(&source)? == source_hash
                    && sha(&partial)? == partial_hash,
                "retry changed a protected image or differed from the good backup"
            );
            fixture.phase(
                proof,
                data,
                &retried,
                "space-check",
                &format!("retried-{op}"),
            )?;
            fixture.phase(proof, data, &source, "space-retire", op)?;
            proof["maintenance_cases"].as_array_mut().unwrap().push(json!({"operation":op,"initial_ballast_bytes":filled,
                "copy_headroom":HEADROOM,"free_after_copy":remaining,"free_at_fresh_opens":0,"free_after_removal":freed,
                "failure":failure,"full_source":full_source,"full_backup":full_backup,"retry":retry,"partial_sha256":partial_hash}));
            write_proof(fixture.output, proof)?;
        }
        fixture.phase(proof, data, &source, "space-advance", "advanced")?;
        fixture.phase(proof, data, &source, "space-check", "advanced-source")?;
        fixture.phase(proof, data, &backup, "space-check", "final-backup")?;
        ensure!(
            sha(&source)? != source_hash && sha(&backup)? == backup_hash,
            "final acknowledged commit or preserved backup"
        );
        verified_copy(
            &source,
            &fixture.output.join("advanced.wdb"),
            fs::metadata(&source)?.len(),
        )?;
        verified_copy(&backup, &fixture.output.join("backup.wdb"), COPY_BYTES)?;
        Ok(())
    }

    pub fn run(run: &Run, opts: Options) -> Result<()> {
        ensure!(
            opts.source.len() == 40
                && opts
                    .source
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "exact source SHA-1 required"
        );
        let output = admission(&run.root, &opts.output)?;
        let binary = dunce::canonicalize(run.root.join(opts.fixture))?;
        ensure!(
            binary.is_file() && !binary.starts_with(&output),
            "fixture must be an existing binary outside output"
        );
        let original_hash = sha(&binary)?;
        fs::create_dir_all(output.parent().context("output parent")?)?;
        fs::create_dir(&output)?;
        let runs = output.join("runs");
        fs::create_dir(&runs)?;
        let copy = output.join("fixture.exe");
        verified_copy(&binary, &copy, fs::metadata(&binary)?.len())?;
        let env: BTreeMap<_, _> = run
            .env
            .iter()
            .filter(|(k, _)| {
                matches!(
                    k.to_string_lossy().to_ascii_uppercase().as_str(),
                    "SYSTEMROOT" | "WINDIR" | "TEMP" | "TMP" | "COMSPEC" | "PATH"
                )
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut fixture = Fixture {
            binary: &copy,
            output: &output,
            runs: &runs,
            source: &opts.source,
            env: env.clone(),
            processes: BTreeSet::new(),
        };
        fixture
            .env
            .insert("WEEKEND_PRIVATE_MAINTENANCE".into(), "ntfs-enospc".into());
        let mut proof = json!({"verified":false,"scope":"near-cap copy exhaustion on private fixed NTFS",
            "source_id":opts.source,"fixture_sha256":original_hash,"commands":[],"maintenance_records":[],"maintenance_cases":[]});
        let seed = output.join("seed");
        let result = (|| -> Result<()> {
            let fixture_sources = run.root.join("tools/resource-envelope");
            let inputs = [
                "src/compound.rs",
                "src/space.rs",
                "Cargo.toml",
                "Cargo.lock",
            ]
            .into_iter()
            .map(|name| Ok((name, sha(&fixture_sources.join(name))?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
            proof["fixture_inputs_sha256"] = json!(inputs);
            proof["host_volume"] = crate::windows_process::volume(&output)?;
            let (stdout, pid) =
                fixture.command(&mut proof, vec![path(&seed)], "near-cap-seed", false)?;
            let rows = rows(&stdout)?;
            oracle::compound(&rows, "native", &opts.source)?;
            proof["seed"] = json!({"pid":pid,"rows":rows});
            windows_volume::with_volume(&mut proof, &output, &env, CAPACITY, |proof, data| {
                qualify(&mut fixture, proof, &seed, data)
            })?;
            ensure!(
                sha(&binary)? == original_hash && sha(&copy)? == original_hash,
                "fixture binary changed"
            );
            for (name, digest) in inputs {
                ensure!(
                    sha(&fixture_sources.join(name))? == digest,
                    "fixture source changed"
                );
            }
            for name in ["rejected-backup.wdb", "restored.wdb", "compact.wdb"] {
                fs::remove_file(seed.join(name))?;
            }
            Ok(())
        })();
        proof["verified"] = json!(result.is_ok());
        if let Err(error) = &result {
            proof["failure"] = json!(format!("{error:#}"));
        }
        let inventory = delivery::inventory(&output);
        match &inventory {
            Ok(hashes) => {
                proof["artifact_sha256"] = json!(
                    hashes
                        .iter()
                        .filter(|(name, _)| *name != "evidence.json")
                        .collect::<BTreeMap<_, _>>()
                )
            }
            Err(error) => {
                proof["verified"] = json!(false);
                proof["inventory_failure"] = json!(format!("{error:#}"));
            }
        }
        write_proof(&output, &proof)?;
        result?;
        inventory?;
        println!("{}", output.join("evidence.json").display());
        Ok(())
    }
}

#[cfg(windows)]
pub use native::{Options, run};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntfs_exhaustion_requires_real_partial_write_and_new_compact_destination() {
        let source = "source";
        let mut failure = [
            json!({"stage":"fail","operation":"compact","pid":10,"error":14,"category":"DiskFull",
            "raw":112,"native_operation":"Write","partial_bytes":8*oracle::MIB,"source_unchanged":true,"backup_unchanged":true}),
            oracle::completion("space-fail", source),
        ];
        assert!(verify_phase(&failure, source, "space-fail", "compact", 10).is_ok());
        for (field, wrong) in [
            ("raw", json!(28)),
            ("native_operation", json!("Open")),
            ("partial_bytes", json!(0)),
            ("pid", json!(11)),
        ] {
            let original = std::mem::replace(&mut failure[0][field], wrong);
            assert!(verify_phase(&failure, source, "space-fail", "compact", 10).is_err());
            failure[0][field] = original;
        }
        let mut retry = [
            json!({"stage":"retry","operation":"compact","pid":12,"partial_rejected":true,
            "existing_rejected":true,"partial_preserved":true,"matches_backup":true}),
            oracle::completion("space-retry", source),
        ];
        assert!(verify_phase(&retry, source, "space-retry", "compact", 12).is_ok());
        retry[0]["existing_rejected"] = json!(false);
        assert!(verify_phase(&retry, source, "space-retry", "compact", 12).is_err());
    }
}
