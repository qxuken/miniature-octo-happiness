//! Independent publication and recovery oracle for the delivered DLL.
#[cfg(any(test, windows))]
use serde_json::{Value, json};

#[cfg(any(test, windows))]
#[derive(Clone, Copy)]
enum Phase {
    Exercise,
    VerifyFull,
    Recover,
}

#[cfg(any(test, windows))]
fn expected(mode: u8, phase: Phase) -> Value {
    match phase {
        Phase::Recover => {
            let reopened = if matches!(mode, 0 | 2 | 3) { 4 } else { 3 };
            json!({"mode":mode,"reopened":reopened,"final":4,"retained_revision":2,"verified":true})
        }
        Phase::VerifyFull => json!({"mode":6,"reopened":3,"retained_revision":2,"verified":true}),
        Phase::Exercise => {
            let (status, syncs, root_writes, witness_writes, native_errno) = match mode {
                0 => (0, 3, 1, 1, 0),
                1 => (14, 1, 0, 0, 1117),
                2 => (15, 2, 1, 0, 1117),
                3 => (15, 3, 1, 1, 1117),
                4 => (15, 1, 1, 0, 1117),
                5 | 6 => (14, 0, 0, 0, 112),
                _ => unreachable!("fixture mode"),
            };
            json!({"mode":mode,"status":status,"visible":if mode==0 {4} else {3},
                "hits":u8::from((1..=5).contains(&mode)),"syncs":syncs,"root_writes":root_writes,
                "witness_writes":witness_writes,"native_errno":native_errno,"verified":true})
        }
    }
}

#[cfg(any(test, windows))]
fn verify(row: &Value, mode: u8, phase: Phase) -> anyhow::Result<()> {
    anyhow::ensure!(
        (!matches!(phase, Phase::VerifyFull) || mode == 6) && *row == expected(mode, phase),
        "delivered fault outcome mismatch"
    );
    Ok(())
}

#[cfg(windows)]
mod native {
    use super::{Phase, verify};
    use crate::{
        delivery,
        support::{path, sha},
        windows_trace::{attestation, call, compiler_command},
        windows_volume,
    };
    use anyhow::{Context, Result, ensure};
    use serde_json::{Value, json};
    use std::{
        collections::BTreeMap,
        ffi::{OsStr, OsString},
        fs,
        path::Path,
    };

    pub(crate) struct Environment<'a> {
        pub output: &'a Path,
        pub input: &'a Path,
        pub package: &'a Path,
        pub compiler: &'a OsStr,
        pub build: &'a BTreeMap<OsString, OsString>,
        pub runtime: &'a BTreeMap<OsString, OsString>,
    }

    struct Consumer<'a> {
        environment: &'a Environment<'a>,
        executable: &'a Path,
        dll: &'a Path,
        digest: &'a str,
        source: &'a str,
    }

    impl Consumer<'_> {
        fn run(
            &self,
            proof: &mut Value,
            dir: &Path,
            data: &Path,
            mode: u8,
            phase: Phase,
            disabled: bool,
        ) -> Result<Value> {
            let (label, argument) = match phase {
                Phase::Exercise => ("exercise", mode.to_string()),
                Phase::VerifyFull => ("verify-full", "verify-full".into()),
                Phase::Recover => ("recover", format!("recover-{mode}")),
            };
            let before = dir.join(format!("{label}-before.json"));
            let after = dir.join(format!("{label}-after.json"));
            let report = call(
                proof,
                self.environment.output,
                vec![
                    path(self.executable),
                    argument.into(),
                    path(data),
                    self.source.into(),
                    path(self.dll),
                    self.digest.into(),
                    path(&before),
                    path(&after),
                ],
                dir,
                label,
                self.environment.runtime,
                disabled,
            )?;
            let pid = report.pid.context("fault consumer PID missing")?;
            let loaded = self.dll.to_str().context("UTF-8 DLL path")?;
            attestation(
                &delivery::read_json(&before)?,
                "before",
                loaded,
                self.digest,
                self.source,
                3,
                pid,
            )?;
            if disabled {
                ensure!(
                    report.exit.is_some_and(|code| code != 0)
                        && !after.exists()
                        && fs::read_to_string(dir.join("exercise.stderr"))?
                            .contains("status==expected"),
                    "disabled fault control missed the expected assertion"
                );
                return Ok(json!({"disabled":true,"exit":report.exit,"pid":pid}));
            }
            attestation(
                &delivery::read_json(&after)?,
                "after",
                loaded,
                self.digest,
                self.source,
                3,
                pid,
            )?;
            let row = delivery::read_json(&dir.join(format!("{label}.stdout")))?;
            verify(&row, mode, phase)?;
            ensure!(
                data.join("database.wdb").is_file(),
                "fault case database missing"
            );
            Ok(json!({"pid":pid,"observation":row}))
        }
    }

    fn disk_full(proof: &mut Value, driver: &Consumer<'_>, build: &Path) -> Result<()> {
        let dir = build.join("disk-full");
        fs::create_dir(&dir)?;
        windows_volume::with_volume(
            proof,
            driver.environment.output,
            driver.environment.runtime,
            windows_volume::Capacity::Append,
            |proof, data| {
                let exercise = driver.run(proof, &dir, data, 6, Phase::Exercise, false)?;
                let full = windows_volume::space(data, windows_volume::Capacity::Append)?;
                ensure!(
                    windows_volume::full(full),
                    "private NTFS volume still has a free allocation unit"
                );
                let database = data.join("database.wdb");
                let exhausted = sha(&database)?;
                let verify_full = driver.run(proof, &dir, data, 6, Phase::VerifyFull, false)?;
                let after = windows_volume::space(data, windows_volume::Capacity::Append)?;
                ensure!(
                    windows_volume::full(after) && sha(&database)? == exhausted,
                    "fresh open under exhaustion changed the database or freed space"
                );
                fs::remove_file(data.join("ballast"))?;
                let freed = windows_volume::space(data, windows_volume::Capacity::Append)?;
                ensure!(
                    freed > 32 * 1024 * 1024,
                    "ballast removal did not restore headroom"
                );
                let recovery = driver.run(proof, &dir, data, 6, Phase::Recover, false)?;
                ensure!(
                    exercise["pid"] != verify_full["pid"]
                        && exercise["pid"] != recovery["pid"]
                        && verify_full["pid"] != recovery["pid"],
                    "fresh reopen process identity required"
                );
                let final_digest = sha(&database)?;
                let preserved = dir.join("database.wdb");
                fs::copy(&database, &preserved)?;
                ensure!(
                    exhausted != final_digest && sha(&preserved)? == final_digest,
                    "retry and archived database bytes"
                );
                proof["disk_full"] = json!({"exercise":exercise,"reopen_while_full":verify_full,"retry":recovery,
                "free_on_failure":full,"free_after_read_only_reopen":after,"free_after_removal":freed,
                "full_database_sha256":exhausted,"retry_database_sha256":final_digest});
                Ok(())
            },
        )
    }

    pub(crate) fn run(
        proof: &mut Value,
        env: Environment<'_>,
        source: &str,
        exhaust: bool,
    ) -> Result<()> {
        let build = env.output.join("faults");
        fs::create_dir(&build)?;
        let dll = build.join("weekend_db.dll");
        fs::copy(env.package.join("weekend_db.dll"), &dll)?;
        let digest = sha(&dll)?;
        let exe = build.join("consumer.exe");
        let disabled = build.join("disabled.exe");
        for (consumer, disable) in [(&exe, false), (&disabled, true)] {
            if exhaust && disable {
                break;
            }
            let mut argv = compiler_command(env.compiler, false, env.input, env.package, consumer);
            argv.insert(1, "/DWDB_WINDOWS_FAULTS".into());
            if disable {
                argv.insert(1, "/DWDB_DISABLE_FAULTS".into());
            }
            call(
                proof,
                env.output,
                argv,
                &build,
                if disable {
                    "compile-disabled"
                } else {
                    "compile"
                },
                env.build,
                false,
            )?;
        }
        let exe_hash = sha(&exe)?;
        let driver = Consumer {
            environment: &env,
            executable: &exe,
            dll: &dll,
            digest: &digest,
            source,
        };
        if exhaust {
            disk_full(proof, &driver, &build)?;
        } else {
            let disabled_hash = sha(&disabled)?;
            let mut cases = Vec::new();
            for mode in 0..6 {
                let dir = build.join(format!("mode-{mode}"));
                fs::create_dir(&dir)?;
                let exercise = driver.run(proof, &dir, &dir, mode, Phase::Exercise, false)?;
                let recovery = driver.run(proof, &dir, &dir, mode, Phase::Recover, false)?;
                ensure!(
                    exercise["pid"] != recovery["pid"],
                    "recovery process identity repeated"
                );
                cases.push(json!({"mode":mode,"exercise":exercise,"recovery":recovery}));
            }
            let control = build.join("disabled");
            fs::create_dir(&control)?;
            let negative = Consumer {
                executable: &disabled,
                ..driver
            };
            proof["disabled_injection"] =
                negative.run(proof, &control, &control, 1, Phase::Exercise, true)?;
            ensure!(sha(&disabled)? == disabled_hash, "disabled control changed");
            proof["outcomes"] = json!(cases);
            proof["disabled_consumer_sha256"] = json!(disabled_hash);
        }
        ensure!(
            sha(&dll)? == digest
                && sha(&env.package.join("weekend_db.dll"))? == digest
                && sha(&exe)? == exe_hash,
            "runtime DLL or consumer changed"
        );
        proof["dll_sha256"] = json!(digest);
        proof["consumer_sha256"] = json!(exe_hash);
        Ok(())
    }
}

#[cfg(windows)]
pub(super) use native::{Environment, run};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_outcome_and_fresh_process_revision_are_both_required() {
        for mode in 0..6 {
            for phase in [Phase::Exercise, Phase::Recover] {
                let mut row = expected(mode, phase);
                verify(&row, mode, phase).unwrap();
                row["verified"] = json!(1);
                assert!(verify(&row, mode, phase).is_err());
            }
        }
        let mut full = expected(5, Phase::Exercise);
        full["native_errno"] = json!(28);
        assert!(verify(&full, 5, Phase::Exercise).is_err());
        assert!(verify(&expected(2, Phase::Recover), 2, Phase::Exercise).is_err());
        assert!(verify(&expected(2, Phase::Recover), 1, Phase::Recover).is_err());
    }

    #[test]
    fn actual_exhaustion_requires_no_injection_and_a_distinct_read_only_observation() {
        let mut exercise = expected(6, Phase::Exercise);
        verify(&exercise, 6, Phase::Exercise).unwrap();
        exercise["hits"] = json!(1);
        assert!(verify(&exercise, 6, Phase::Exercise).is_err());
        let full = expected(6, Phase::VerifyFull);
        verify(&full, 6, Phase::VerifyFull).unwrap();
        assert!(verify(&full, 5, Phase::VerifyFull).is_err());
        assert!(verify(&full, 6, Phase::Recover).is_err());
    }
}
