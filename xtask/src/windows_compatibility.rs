//! Native process adapter for the frozen compatibility oracles.
use crate::{
    qualification_net::{loopback_address, ready_line},
    support::{Run, cmd, path, sha},
    trace_process::{self, Request},
    windows_process,
};
use anyhow::{Context, Result, anyhow, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

const INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "xtask/Cargo.toml",
    "xtask/src/check.rs",
    "xtask/src/compatibility.rs",
    "xtask/src/coordinator_recovery.rs",
    "xtask/src/msrv.rs",
    "xtask/src/incident_recovery.rs",
    "xtask/src/qualification_net.rs",
    "xtask/src/replica_progress.rs",
    "xtask/src/snapshot_proof.rs",
    "xtask/src/snapshot_upgrade.rs",
    "xtask/src/study_upgrade.rs",
    "xtask/src/source_archive.rs",
    "xtask/src/support.rs",
    "xtask/src/trace_process.rs",
    "xtask/src/windows_compatibility.rs",
    "xtask/src/windows_process.rs",
    "tools/compatibility-driver.rs",
    "tools/compatibility-snapshot.rs",
    "tools/compatibility-typed-v1.rs",
    "tools/compatibility-typed-v2.rs",
    "tools/replica-progress-fixture/Cargo.lock",
    "tools/replica-progress-fixture/Cargo.toml",
    "tools/replica-progress-fixture/src/main.rs",
    "tools/snapshot-cli-upgrade-fixture.rs",
    "tools/snapshot-test-keygen.rs",
    "tools/study-cli-upgrade-fixture.rs",
    "tools/admin.rs",
    "tools/incident-recovery/Cargo.toml",
    "tools/incident-recovery/Cargo.lock",
    "tools/incident-recovery/src/main.rs",
    "tests/fixtures/logical-v1.bin",
];

struct Capture {
    record: Value,
    output: Result<String>,
}

#[derive(Debug, Clone, Copy)]
enum Exit<'a> {
    Code(i64),
    Rejected(&'a str),
    StoppedOrSuccess,
}

struct Invocation<'a> {
    args: Vec<OsString>,
    name: &'a str,
    cwd: PathBuf,
    stdout: PathBuf,
    stderr: PathBuf,
    timeout: f64,
    maximum: u64,
}

struct StopOnUnwind<'a>(&'a AtomicBool);
impl Drop for StopOnUnwind<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.store(true, Ordering::Relaxed);
        }
    }
}

fn input_hashes(root: &Path) -> Result<BTreeMap<&'static str, String>> {
    INPUTS
        .iter()
        .map(|&name| Ok((name, sha(&root.join(name))?)))
        .collect()
}

pub(super) struct Campaign<'a> {
    run: &'a Run,
    pub output: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub evidence: Value,
    maximum: u64,
}

impl<'a> Campaign<'a> {
    pub fn new(run: &'a Run, output: &Path, scope: &str) -> Result<Self> {
        let output = trace_process::admission(&run.root, output)?;
        fs::create_dir_all(output.parent().context("output parent")?)?;
        fs::create_dir(&output).context("new evidence directory")?;
        Self::from_directory(run, output, scope)
    }

    pub fn new_private(run: &'a Run, output: &Path, scope: &str) -> Result<Self> {
        let output = trace_process::admission(&run.root, output)?;
        fs::create_dir_all(output.parent().context("output parent")?)?;
        let sid = windows_process::current_user_sid(&run.root, &run.env)?;
        windows_process::create_private_directory(&output, &sid)?;
        let mut campaign = Self::from_directory(run, output, scope)?;
        campaign.evidence["output_creation"] = json!({"owner_sid":sid,"acl":"protected, inheritable current-user full control at creation"});
        campaign.save()?;
        Ok(campaign)
    }

    fn from_directory(run: &'a Run, output: PathBuf, scope: &str) -> Result<Self> {
        let mut campaign = Self {
            run,
            evidence: json!({
                "passed":false,"verified":false,"scope":scope,"commands":[],
                "source_hashes":input_hashes(&run.root)?,
                "runner_sha256":sha(&std::env::current_exe()?)?,
                "volume":windows_process::volume(&output)?,
                "git_identity_available":true,
            }),
            env: run.env.clone(),
            output,
            maximum: 8 * 1024 * 1024,
        };
        campaign.save()?;
        let source = campaign.command(cmd(["git", "rev-parse", "HEAD"]), "source", None, 30.)?;
        let dirty = campaign.command(cmd(["git", "status", "--porcelain"]), "dirty", None, 30.)?;
        campaign.evidence["source"] = json!(source);
        campaign.evidence["dirty"] = json!(!dirty.is_empty());
        campaign.save()?;
        Ok(campaign)
    }

    pub fn save(&self) -> Result<()> {
        trace_process::write_proof(&self.output, &self.evidence)
    }

    pub fn limits(&mut self, maximum: u64, file_limit: Option<u64>) -> Result<()> {
        ensure!(
            maximum > 0 && maximum <= 64 * 1024 * 1024 && file_limit.is_none(),
            "invalid Windows campaign output limits"
        );
        self.maximum = maximum;
        Ok(())
    }

    pub fn restrict_directories(&mut self, directories: &[(&str, &Path)]) -> Result<()> {
        let identity = self.command(
            cmd(["whoami.exe", "/user", "/fo", "csv", "/nh"]),
            "credential-owner",
            None,
            30.,
        )?;
        let sid = identity
            .rsplit_once(',')
            .context("Windows user SID missing")?
            .1
            .trim_matches('"');
        let numbers = sid
            .strip_prefix("S-1-")
            .context("invalid Windows user SID")?;
        ensure!(
            numbers
                .split('-')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
            "invalid Windows user SID"
        );
        let grant = format!("*{sid}:(OI)(CI)F");
        for (label, directory) in directories {
            self.command(
                vec![
                    "icacls.exe".into(),
                    path(directory),
                    "/inheritance:r".into(),
                    "/grant:r".into(),
                    grant.clone().into(),
                ],
                &format!("{label}-restrict-acl"),
                None,
                30.,
            )?;
        }
        Ok(())
    }

    pub fn command(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        expected: Option<&str>,
        timeout: f64,
    ) -> Result<String> {
        self.command_at(args, name, expected, timeout, &self.run.root)
    }

    pub fn command_at(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        expected: Option<&str>,
        timeout: f64,
        cwd: &Path,
    ) -> Result<String> {
        let expected = expected.map_or(Exit::Code(0), Exit::Rejected);
        self.execute(args, name, expected, timeout, cwd)
    }

    pub fn command_exit(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        code: i32,
        timeout: f64,
    ) -> Result<String> {
        self.execute(
            args,
            name,
            Exit::Code(i64::from(code)),
            timeout,
            &self.run.root,
        )
    }

    fn execute(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        expected: Exit<'_>,
        timeout: f64,
        cwd: &Path,
    ) -> Result<String> {
        let captured = self
            .invocation(args, name, timeout, cwd)?
            .capture(&self.env, expected, None)?;
        self.record(captured)
    }

    pub fn pair(&mut self, commands: [(&str, Vec<OsString>); 2], timeout: f64) -> Result<()> {
        let [(first, a), (second, b)] = commands;
        ensure!(
            !first.eq_ignore_ascii_case(second),
            "parallel command names must differ"
        );
        let a = self.invocation(a, first, timeout, &self.run.root)?;
        let b = self.invocation(b, second, timeout, &self.run.root)?;
        let (one, two) = thread::scope(|scope| {
            let worker = scope.spawn(|| a.capture(&self.env, Exit::Code(0), None));
            let two = b.capture(&self.env, Exit::Code(0), None);
            let one = worker
                .join()
                .unwrap_or_else(|_| Err(anyhow!("compatibility supervisor thread panicked")));
            (one, two)
        });
        let one = one.and_then(|capture| self.record(capture));
        let two = two.and_then(|capture| self.record(capture));
        one?;
        two?;
        Ok(())
    }

    pub fn loopback<T>(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        (readiness, timeout): (Duration, f64),
        wait_for_exit: bool,
        action: impl FnOnce(&mut Self, SocketAddr) -> Result<T>,
    ) -> Result<T> {
        let invocation = self.invocation(args, name, timeout, &self.run.root)?;
        let env = self.env.clone();
        let stop = AtomicBool::new(false);
        let (outcome, captured) = thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let expected = if wait_for_exit {
                    Exit::Code(0)
                } else {
                    Exit::StoppedOrSuccess
                };
                invocation.capture(&env, expected, Some(&stop))
            });
            let _stop_on_unwind = StopOnUnwind(&stop);
            let outcome = (|| {
                let started = Instant::now();
                let address = loop {
                    ensure!(!worker.is_finished(), "server exited before readiness");
                    if let Some(line) = ready_line(&invocation.stdout)? {
                        break loopback_address(&line)?;
                    }
                    ensure!(started.elapsed() < readiness, "server readiness deadline");
                    thread::sleep(Duration::from_millis(15));
                };
                action(self, address)
            })();
            if outcome.is_err() || !wait_for_exit {
                stop.store(true, Ordering::Relaxed);
            }
            let captured = worker
                .join()
                .unwrap_or_else(|_| Err(anyhow!("server supervisor thread panicked")));
            (outcome, captured)
        });
        let observed = captured.and_then(|capture| self.record(capture));
        match (outcome, observed) {
            (Ok(value), Ok(_)) => Ok(value),
            (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error),
            (Err(client), Err(server)) => anyhow::bail!("{client:#}; {server:#}"),
        }
    }

    fn invocation<'b>(
        &self,
        args: Vec<OsString>,
        name: &'b str,
        timeout: f64,
        cwd: &Path,
    ) -> Result<Invocation<'b>> {
        ensure!(
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid compatibility log label"
        );
        Ok(Invocation {
            args,
            name,
            timeout,
            maximum: self.maximum,
            cwd: cwd.to_owned(),
            stdout: self.output.join(format!("{name}.stdout")),
            stderr: self.output.join(format!("{name}.stderr")),
        })
    }

    fn record(&mut self, capture: Capture) -> Result<String> {
        self.evidence["commands"]
            .as_array_mut()
            .unwrap()
            .push(capture.record);
        self.save()?;
        capture.output
    }

    pub fn finish(mut self, result: Result<()>) -> Result<()> {
        let result = result.and_then(|()| {
            ensure!(
                self.evidence["source_hashes"] == json!(input_hashes(&self.run.root)?),
                "compatibility qualification inputs changed"
            );
            Ok(())
        });
        self.evidence["passed"] = json!(result.is_ok());
        self.evidence["verified"] = json!(result.is_ok());
        if let Err(error) = &result {
            self.evidence["error"] = json!(format!("{error:#}"));
        }
        self.save()?;
        result
    }
}

impl Invocation<'_> {
    fn capture(
        &self,
        env: &BTreeMap<OsString, OsString>,
        expected: Exit<'_>,
        stop: Option<&AtomicBool>,
    ) -> Result<Capture> {
        let argv = self
            .args
            .iter()
            .map(|arg| arg.to_str().context("UTF-8 recorded command"))
            .collect::<Result<Vec<_>>>()?;
        let request = Request {
            argv: &self.args,
            cwd: &self.cwd,
            env,
            stdout: &self.stdout,
            stderr: &self.stderr,
            timeout: self.timeout,
            maximum: self.maximum,
        };
        let result = match stop {
            Some(flag) => trace_process::run_stoppable(&request, flag),
            None => trace_process::run(&request),
        };
        let passed = result.as_ref().is_ok_and(|report| match expected {
            Exit::Code(code) => report.qualifies(true) && report.exit == Some(code),
            Exit::Rejected(marker) => {
                report.qualifies(true)
                    && report.exit == Some(1)
                    && fs::read_to_string(&self.stderr).is_ok_and(|text| text.contains(marker))
            }
            Exit::StoppedOrSuccess => report.qualifies(false) || report.stopped_cleanly(),
        });
        let code = if let Exit::Code(code) = expected {
            Some(code)
        } else {
            None
        };
        let marker = if let Exit::Rejected(marker) = expected {
            Some(marker)
        } else {
            None
        };
        let name = self.name;
        let mut record = json!({
            "label":name,"argv":argv,"cwd":self.cwd,"expected_exit_code":code,"expected_assertion":marker,"passed":passed,
            "stdout_sha256":sha(&self.stdout).ok(),"stderr_sha256":sha(&self.stderr).ok(),
        });
        match &result {
            Ok(report) => {
                record["pid"] = json!(report.pid);
                record["exit_code"] = json!(report.exit);
                record["cleanup_verified"] = json!(report.cleanup_verified);
                record["report"] = json!(report);
            }
            Err(error) => record["error"] = json!(format!("{error:#}")),
        }
        let output = result.and_then(|report| {
            ensure!(
                passed,
                "compatibility command {name} expected {expected:?}: {report:?}"
            );
            Ok(fs::read_to_string(&self.stdout)?.trim().into())
        });
        Ok(Capture { record, output })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_campaign_protects_initialization_failure_and_inherited_files() {
        let mut scratch =
            crate::support::Scratch::new(&std::env::temp_dir(), "private-windows-campaign")
                .unwrap();
        let root = scratch.path().join("missing-inputs");
        fs::create_dir(&root).unwrap();
        let run = Run::new(root);
        let output = scratch.path().join("evidence");
        assert!(Campaign::new_private(&run, &output, "private initialization failure").is_err());
        assert!(!output.join("evidence.json").exists());
        let sid = windows_process::current_user_sid(&run.root, &run.env).unwrap();
        fs::write(output.join("child"), b"owned data").unwrap();
        let mut env = run.env.clone();
        // The CI parent is pwsh. Let Windows PowerShell initialize its own
        // module paths rather than trying to load pwsh's incompatible modules.
        env.retain(|name, _| !name.to_string_lossy().eq_ignore_ascii_case("PSModulePath"));
        env.insert("WEEKEND_PRIVATE_OUTPUT_TEST".into(), path(&output));
        env.insert("WEEKEND_PRIVATE_OWNER_TEST".into(), path(&sid));
        let script = r#"
$ErrorActionPreference='Stop'
try {
    $sid=$env:WEEKEND_PRIVATE_OWNER_TEST
    $p=$env:WEEKEND_PRIVATE_OUTPUT_TEST
    $a=Get-Acl -LiteralPath $p
    [Console]::Out.WriteLine('ACL protected='+$a.AreAccessRulesProtected+' owner='+$a.GetOwner([System.Security.Principal.SecurityIdentifier]).Value+' expected='+$sid)
    if(-not $a.AreAccessRulesProtected -or $a.GetOwner([System.Security.Principal.SecurityIdentifier]).Value -ne $sid){throw 'directory protection or owner'}
    foreach($q in @($p, (Join-Path $p 'child'))){
        $acl=Get-Acl -LiteralPath $q
        $r=@($acl.GetAccessRules($true,$true,[System.Security.Principal.SecurityIdentifier]))
        [Console]::Out.WriteLine('ACL path='+$q+' rules='+$r.Count)
        foreach($rule in $r){[Console]::Out.WriteLine('ACL sid='+$rule.IdentityReference.Value+' type='+$rule.AccessControlType+' rights='+$rule.FileSystemRights+' inherited='+$rule.IsInherited)}
        $full=[System.Security.AccessControl.FileSystemRights]::FullControl
        if($r.Count -ne 1 -or $r[0].IdentityReference.Value -ne $sid -or $r[0].AccessControlType -ne [System.Security.AccessControl.AccessControlType]::Allow -or ($r[0].FileSystemRights -band $full) -ne $full){throw 'unexpected access rule'}
    }
    [Console]::Out.WriteLine('verified private ACL')
} catch {
    [Console]::Out.WriteLine('ACL probe error='+$_.Exception.GetType().FullName+': '+$_.Exception.Message)
}
exit 0
"#;
        let bytes = windows_process::capture_private(
            &cmd([
                "powershell.exe",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ]),
            &run.root,
            &env,
            30.,
        )
        .unwrap();
        let observation = String::from_utf8(bytes).unwrap();
        assert_eq!(
            observation.lines().last(),
            Some("verified private ACL"),
            "{observation}"
        );
        assert!(windows_process::create_private_directory(&output, &sid).is_err());
        assert_eq!(fs::read(output.join("child")).unwrap(), b"owned data");
        let invalid = scratch.path().join("invalid-owner");
        assert!(windows_process::create_private_directory(&invalid, "S-1-5-21;D:").is_err());
        assert!(!invalid.exists());
        scratch.close().unwrap();
    }
    use crate::support::Scratch;

    fn detached(run: &Run) -> Campaign<'_> {
        Campaign {
            run,
            output: run.root.clone(),
            env: run.env.clone(),
            evidence: json!({"commands":[]}),
            maximum: 8 * 1024 * 1024,
        }
    }

    #[test]
    fn configured_output_limit_keeps_overflow_and_cleanup_as_failure() {
        let scratch = Scratch::new(&std::env::temp_dir(), "windows-campaign-limit").unwrap();
        let run = Run::new(scratch.path().to_owned());
        let mut campaign = detached(&run);
        assert!(campaign.limits(0, None).is_err());
        assert!(campaign.limits(64 * 1024 * 1024 + 1, None).is_err());
        assert!(campaign.limits(128, Some(1)).is_err());
        campaign.limits(128, None).unwrap();
        assert!(
            campaign
                .command(
                    cmd([
                        "cmd.exe",
                        "/D",
                        "/Q",
                        "/C",
                        "for /L %i in (1,1,4096) do @echo payload"
                    ]),
                    "overflow",
                    None,
                    15.
                )
                .is_err()
        );
        let row = &campaign.evidence["commands"][0];
        assert_eq!(row["passed"], false);
        assert_eq!(row["report"]["output_exceeded"], true);
        assert_eq!(row["cleanup_verified"], true);
    }

    #[test]
    fn parallel_command_failure_keeps_both_retired_jobs() {
        let scratch = Scratch::new(&std::env::temp_dir(), "windows-compatibility-pair").unwrap();
        let run = Run::new(scratch.path().to_owned());
        let mut campaign = detached(&run);
        let first = cmd(["cmd.exe", "/D", "/Q", "/C", "exit /b 9"]);
        let second = cmd(["cmd.exe", "/D", "/Q", "/C", "exit /b 0"]);
        assert!(
            campaign
                .pair([("one", first), ("two", second)], 15.)
                .is_err()
        );
        let proof = crate::delivery::read_json(&run.root.join("evidence.json")).unwrap();
        let rows = proof["commands"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["exit_code"], 9);
        assert_eq!(rows[1]["exit_code"], 0);
        assert!(rows.iter().all(|row| row["cleanup_verified"] == true));
    }

    #[test]
    fn failed_action_retires_the_ready_server_job() {
        let scratch = Scratch::new(&std::env::temp_dir(), "windows-compatibility-server").unwrap();
        let run = Run::new(scratch.path().to_owned());
        let mut campaign = detached(&run);
        let server = cmd([
            "cmd.exe",
            "/D",
            "/Q",
            "/C",
            "echo ready 127.0.0.1:45000&ping -n 30 127.0.0.1 >nul",
        ]);
        let result: Result<()> = campaign.loopback(
            server,
            "server",
            (Duration::from_secs(5), 20.),
            false,
            |_, addr| {
                assert_eq!(addr, "127.0.0.1:45000".parse().unwrap());
                anyhow::bail!("client negative control")
            },
        );
        assert_eq!(result.unwrap_err().to_string(), "client negative control");
        let row = &campaign.evidence["commands"][0];
        assert_eq!(row["passed"], true);
        assert_eq!(row["cleanup_verified"], true);
        assert_eq!(row["report"]["stopped"], true);
    }
}
