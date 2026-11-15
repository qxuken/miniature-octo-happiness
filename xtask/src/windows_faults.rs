//! Independent publication and recovery oracle for the delivered DLL.
#[cfg(any(test, windows))]
use serde_json::{Value, json};

#[cfg(any(test, windows))]
fn expected(mode: u8, recover: bool) -> Value {
    if recover {
        let reopened = if matches!(mode, 0 | 2 | 3) { 4 } else { 3 };
        json!({"mode":mode,"reopened":reopened,"final":4,"retained_revision":2,"verified":true})
    } else {
        let (status, syncs, root_writes, witness_writes, native_errno) = match mode {
            0 => (0, 3, 1, 1, 0),
            1 => (14, 1, 0, 0, 1117),
            2 => (15, 2, 1, 0, 1117),
            3 => (15, 3, 1, 1, 1117),
            4 => (15, 1, 1, 0, 1117),
            5 => (14, 0, 0, 0, 112),
            _ => unreachable!("fixture mode"),
        };
        json!({"mode":mode,"status":status,"visible":if mode==0 {4} else {3},
            "hits":u8::from(mode!=0),"syncs":syncs,"root_writes":root_writes,
            "witness_writes":witness_writes,"native_errno":native_errno,"verified":true})
    }
}

#[cfg(any(test, windows))]
fn verify(row: &Value, mode: u8, recover: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        *row == expected(mode, recover),
        "delivered fault outcome mismatch"
    );
    Ok(())
}

#[cfg(windows)]
mod native {
    use super::verify;
    use crate::{
        delivery,
        support::{path, sha},
        windows_trace::{attestation, call, compiler_command},
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
        dll: &'a Path,
        digest: &'a str,
        source: &'a str,
    }

    impl Consumer<'_> {
        fn run(
            &self,
            proof: &mut Value,
            exe: &Path,
            dir: &Path,
            mode: u8,
            recover: bool,
            disabled: bool,
        ) -> Result<Value> {
            let label = if recover { "recover" } else { "exercise" };
            let argument = if recover {
                format!("recover-{mode}")
            } else {
                mode.to_string()
            };
            let before = dir.join(format!("{label}-before.json"));
            let after = dir.join(format!("{label}-after.json"));
            let report = call(
                proof,
                self.environment.output,
                vec![
                    path(exe),
                    argument.into(),
                    path(dir),
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
            verify(&row, mode, recover)?;
            ensure!(
                dir.join("database.wdb").is_file(),
                "fault case database missing"
            );
            Ok(json!({"pid":pid,"observation":row}))
        }
    }

    pub(crate) fn run(proof: &mut Value, env: Environment<'_>, source: &str) -> Result<()> {
        let build = env.output.join("faults");
        fs::create_dir(&build)?;
        let dll = build.join("weekend_db.dll");
        fs::copy(env.package.join("weekend_db.dll"), &dll)?;
        let digest = sha(&dll)?;
        let exe = build.join("consumer.exe");
        let disabled = build.join("disabled.exe");
        for (consumer, disable) in [(&exe, false), (&disabled, true)] {
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
        let disabled_hash = sha(&disabled)?;
        let driver = Consumer {
            environment: &env,
            dll: &dll,
            digest: &digest,
            source,
        };
        let mut cases = Vec::new();
        for mode in 0..6 {
            let dir = build.join(format!("mode-{mode}"));
            fs::create_dir(&dir)?;
            let exercise = driver.run(proof, &exe, &dir, mode, false, false)?;
            let recovery = driver.run(proof, &exe, &dir, mode, true, false)?;
            ensure!(
                exercise["pid"] != recovery["pid"],
                "recovery process identity repeated"
            );
            cases.push(json!({"mode":mode,"exercise":exercise,"recovery":recovery}));
        }
        let control = build.join("disabled");
        fs::create_dir(&control)?;
        proof["disabled_injection"] = driver.run(proof, &disabled, &control, 1, false, true)?;
        ensure!(
            sha(&dll)? == digest
                && sha(&env.package.join("weekend_db.dll"))? == digest
                && sha(&exe)? == exe_hash
                && sha(&disabled)? == disabled_hash,
            "runtime DLL or consumer changed"
        );
        proof["outcomes"] = json!(cases);
        proof["dll_sha256"] = json!(digest);
        proof["consumer_sha256"] = json!(exe_hash);
        proof["disabled_consumer_sha256"] = json!(disabled_hash);
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
            for recover in [false, true] {
                let mut row = expected(mode, recover);
                verify(&row, mode, recover).unwrap();
                row["verified"] = json!(1);
                assert!(verify(&row, mode, recover).is_err());
            }
        }
        let mut full = expected(5, false);
        full["native_errno"] = json!(28);
        assert!(verify(&full, 5, false).is_err());
        assert!(verify(&expected(2, true), 2, false).is_err());
        assert!(verify(&expected(2, true), 1, true).is_err());
    }
}
