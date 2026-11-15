//! Source-bound diagnostic campaigns. Expected failures still require verified
//! process retirement and a specific stderr diagnostic, never just a nonzero exit.
use crate::{
    supervise::{self, Request},
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Read},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub fn snapshot(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "include/weekend_db.h",
        ".cargo/config.toml",
        "tools/coverage-fixture.rs",
        "tools/benchmark.rs",
        "tools/compatibility-driver.rs",
        "tools/compatibility-snapshot.rs",
        "tools/compatibility-typed-v1.rs",
        "tools/compatibility-typed-v2.rs",
        "tools/snapshot-cli-upgrade-fixture.rs",
        "tools/monthly_workload.rs",
        "tools/study-cli-upgrade-fixture.rs",
    ] {
        let file = root.join(name);
        if file.is_file() {
            out.insert(name.into(), sha(&file)?);
        }
    }
    for name in [
        "src",
        "crates",
        "apps",
        "examples",
        "tests",
        "fuzz",
        "xtask",
        "probe",
        "tools/atomic-model",
        "tools/serialization-scale",
        "tools/resource-envelope",
        "tools/sqlite-baseline",
        "tools/soak",
        "tools/monthly-workload",
    ] {
        walk(root, &root.join(name), &mut out, false)?;
    }
    Ok(out)
}
fn walk(
    root: &Path,
    directory: &Path,
    out: &mut BTreeMap<String, String>,
    all: bool,
) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let file = entry?.path();
        if file == root.join("probe/build") {
            continue;
        }
        if matches!(
            file.file_name().and_then(|s| s.to_str()),
            Some("target" | "corpus" | "artifacts" | ".git")
        ) {
            continue;
        }
        let kind = fs::symlink_metadata(&file)?.file_type();
        ensure!(!kind.is_symlink(), "source symlink: {}", file.display());
        if kind.is_dir() {
            walk(root, &file, out, all)?;
        } else if kind.is_file()
            && (all
                || file.starts_with(root.join("xtask/tests"))
                || matches!(
                    file.extension().and_then(|s| s.to_str()),
                    Some("rs" | "c" | "h" | "json" | "odin")
                )
                || matches!(
                    file.file_name().and_then(|s| s.to_str()),
                    Some("Cargo.toml" | "Cargo.lock" | "legacy-controls.json")
                ))
        {
            out.insert(
                file.strip_prefix(root)?
                    .to_str()
                    .context("UTF-8 source path")?
                    .replace('\\', "/"),
                sha(&file)?,
            );
        }
    }
    Ok(())
}
pub fn seeds(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    walk(root, &root.join("fuzz/seeds"), &mut out, true)?;
    Ok(out)
}
pub fn absolute(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}
pub fn fresh(root: &Path, path: &Path) -> Result<PathBuf> {
    let path = absolute(root, path);
    // create_dir, unlike create_dir_all, rejects existing and dangling outputs.
    fs::create_dir_all(path.parent().context("output parent")?)?;
    fs::create_dir(&path).with_context(|| format!("new evidence directory {}", path.display()))?;
    Ok(dunce::canonicalize(path)?)
}
pub fn fresh_output(root: &Path, path: &Path) -> Result<PathBuf> {
    let output = absolute(root, path);
    ensure!(
        fs::symlink_metadata(&output)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            && output.parent().is_some_and(Path::is_dir),
        "fresh output with existing parent required"
    );
    crate::notices::resolve(&output)
}
pub struct Campaign<'a> {
    pub run: &'a Run,
    pub output: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub evidence: Value,
    pub sample_rss: bool,
    before: BTreeMap<String, String>,
    maximum_bytes: u64,
    file_limit: Option<u64>,
    cancellation: supervise::Cancellation,
}
struct StopOnUnwind<'a>(&'a AtomicBool);
impl Drop for StopOnUnwind<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.store(true, Ordering::Relaxed);
        }
    }
}
enum ExpectedExit<'a> {
    Success,
    Assertion(&'a str),
    Code(i32),
    StoppedOrSuccess,
}
impl<'a> Campaign<'a> {
    pub fn new(run: &'a Run, output: &Path, scope: &str) -> Result<Self> {
        Self::create(run, output, scope, true)
    }
    /// A frozen worker snapshot has no Git directory. Its exact input hashes
    /// and runner binary are attested, without inventing a clean checkout ID.
    pub fn detached(run: &'a Run, output: &Path, scope: &str) -> Result<Self> {
        Self::create(run, output, scope, false)
    }
    fn create(run: &'a Run, output: &Path, scope: &str, git: bool) -> Result<Self> {
        let output = fresh(&run.root, output)?;
        let before = snapshot(&run.root)?;
        let mut campaign = Self {
            run,
            output,
            env: run.env.clone(),
            before: before.clone(),
            maximum_bytes: 64 * 1024 * 1024,
            file_limit: None,
            cancellation: supervise::Cancellation::new()?,
            sample_rss: false,
            evidence: json!({"passed":false,"scope":scope,"source_hashes":before,"commands":[]}),
        };
        campaign.save()?;
        if git {
            let source =
                campaign.command(cmd(["git", "rev-parse", "HEAD"]), "source", None, 30.)?;
            campaign.evidence["source"] = json!(source.trim());
            let dirty =
                campaign.command(cmd(["git", "status", "--porcelain"]), "dirty", None, 30.)?;
            campaign.evidence["dirty"] = json!(!dirty.trim().is_empty());
        } else {
            campaign.evidence["source"] = Value::Null;
            campaign.evidence["dirty"] = Value::Null;
        }
        campaign.evidence["git_identity_available"] = json!(git);
        campaign.evidence["runner_sha256"] = json!(sha(&std::env::current_exe()?)?);
        campaign.save()?;
        Ok(campaign)
    }
    pub fn save(&self) -> Result<()> {
        let mut bytes = serde_json::to_vec_pretty(&self.evidence)?;
        bytes.push(b'\n');
        fs::write(self.output.join("evidence.json"), bytes)?;
        Ok(())
    }
    pub fn limits(&mut self, log_bytes: u64, file_bytes: Option<u64>) -> Result<()> {
        ensure!(
            log_bytes > 0 && log_bytes <= 64 * 1024 * 1024 && file_bytes.is_none_or(|n| n > 0),
            "invalid campaign output limits"
        );
        self.maximum_bytes = log_bytes;
        self.file_limit = file_bytes;
        Ok(())
    }
    pub fn unchanged(&self) -> Result<bool> {
        Ok(self.before == snapshot(&self.run.root)?)
    }
    pub fn cancelled(&self) -> bool {
        self.cancellation.requested()
    }
    pub fn command(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        expected: Option<&str>,
        timeout: f64,
    ) -> Result<String> {
        self.command_at(args, name, expected, timeout, &self.run.root.clone())
    }
    pub fn command_at(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        expected: Option<&str>,
        timeout: f64,
        cwd: &Path,
    ) -> Result<String> {
        ensure!(!self.cancellation.requested(), "campaign cancelled");
        self.execute(args, name, expected, timeout, cwd)
    }
    pub fn command_exit(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        code: i32,
        timeout: f64,
    ) -> Result<String> {
        ensure!(
            !self.cancelled() && code > 0,
            "invalid expected command exit"
        );
        let request = self.request(args, name, timeout, &self.run.root)?;
        let report = supervise::run(&request);
        self.record(name, &request, report, ExpectedExit::Code(code))
    }
    /// Only resource retirement/absence queries may run after cancellation.
    pub fn retirement_command(&mut self, args: Vec<OsString>, name: &str) -> Result<String> {
        self.execute(args, name, None, 30., &self.run.root.clone())
    }
    fn execute(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        expected: Option<&str>,
        timeout: f64,
        cwd: &Path,
    ) -> Result<String> {
        let request = self.request(args, name, timeout, cwd)?;
        let report = supervise::run(&request);
        self.record(
            name,
            &request,
            report,
            expected.map_or(ExpectedExit::Success, ExpectedExit::Assertion),
        )
    }
    pub fn pair(&mut self, commands: [(&str, Vec<OsString>); 2], timeout: f64) -> Result<()> {
        ensure!(!self.cancellation.requested(), "campaign cancelled");
        let [(first, a), (second, b)] = commands;
        ensure!(first != second, "parallel command names must differ");
        let a = self.request(a, first, timeout, &self.run.root)?;
        let b = self.request(b, second, timeout, &self.run.root)?;
        let (one, two) = std::thread::scope(|scope| {
            let worker = scope.spawn(|| supervise::run(&a));
            let two = supervise::run(&b);
            let one = worker
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("supervisor thread panicked")));
            (one, two)
        });
        let one = self.record(first, &a, one, ExpectedExit::Success);
        let two = self.record(second, &b, two, ExpectedExit::Success);
        one?;
        two?;
        Ok(())
    }
    /// Start a bounded loopback process, observe readiness, then run its client.
    /// A natural exit can be required for finite servers.
    pub fn loopback<T>(
        &mut self,
        arguments: Vec<OsString>,
        label: &str,
        limits: (Duration, f64),
        wait_for_exit: bool,
        action: impl FnOnce(&mut Self, SocketAddr) -> Result<T>,
    ) -> Result<T> {
        self.ready_process(
            arguments,
            label,
            limits,
            wait_for_exit,
            loopback_address,
            action,
        )
    }
    pub fn ready_process<T, R>(
        &mut self,
        arguments: Vec<OsString>,
        label: &str,
        limits: (Duration, f64),
        wait_for_exit: bool,
        parse_ready: impl FnOnce(&str) -> Result<R>,
        action: impl FnOnce(&mut Self, R) -> Result<T>,
    ) -> Result<T> {
        ensure!(!self.cancelled(), "campaign cancelled");
        let (readiness, deadline) = limits;
        let request = self.request(arguments, label, deadline, &self.run.root)?;
        let stop = AtomicBool::new(false);
        let (outcome, report) = std::thread::scope(|scope| {
            let _stop_on_unwind = StopOnUnwind(&stop);
            let worker = scope.spawn(|| supervise::run_stoppable(&request, &stop));
            let outcome = (|| -> Result<T> {
                let started = Instant::now();
                let ready = loop {
                    ensure!(!self.cancelled(), "campaign cancelled");
                    ensure!(!worker.is_finished(), "server exited before readiness");
                    if let Some(line) = ready_line(&request.stdout)? {
                        break parse_ready(&line)?;
                    }
                    ensure!(started.elapsed() < readiness, "server readiness deadline");
                    std::thread::sleep(Duration::from_millis(15));
                };
                action(self, ready)
            })();
            if outcome.is_err() || !wait_for_exit {
                stop.store(true, Ordering::Relaxed);
            }
            let report = worker
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("server supervisor thread panicked")));
            (outcome, report)
        });
        let expected = if outcome.is_err() || !wait_for_exit {
            ExpectedExit::StoppedOrSuccess
        } else {
            ExpectedExit::Success
        };
        let observed = self.record(label, &request, report, expected);
        match (outcome, observed) {
            (Ok(value), Ok(_)) => Ok(value),
            (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error),
            (Err(client), Err(server)) => anyhow::bail!("{client:#}; {server:#}"),
        }
    }
    pub fn request(
        &self,
        args: Vec<OsString>,
        name: &str,
        timeout: f64,
        cwd: &Path,
    ) -> Result<Request> {
        ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c)),
            "invalid log name"
        );
        let strings = args
            .into_iter()
            .map(|s| {
                s.into_string()
                    .map_err(|_| anyhow::anyhow!("UTF-8 command"))
            })
            .collect::<Result<Vec<_>>>()?;
        let env = self
            .env
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.to_str().context("UTF-8 env name")?.to_owned(),
                    v.to_str().context("UTF-8 env value")?.to_owned(),
                ))
            })
            .collect::<Result<_>>()?;
        eprintln!("+ {}", strings.join(" "));
        let stdout = self.output.join(format!("{name}.stdout.log"));
        let stderr = self.output.join(format!("{name}.stderr.log"));
        Ok(Request {
            argv: strings,
            cwd: Some(cwd.to_owned()),
            env: Some(env),
            stdout,
            stderr,
            report: Some(self.output.join(format!("{name}.process.json"))),
            timeout_seconds: timeout,
            maximum_bytes: self.maximum_bytes,
            exclusive_limit: false,
            file_limit: self.file_limit,
            journal: None,
            sample_rss: self.sample_rss,
        })
    }
    fn record(
        &mut self,
        name: &str,
        request: &Request,
        report: Result<supervise::Report>,
        expected: ExpectedExit<'_>,
    ) -> Result<String> {
        let stdout = &request.stdout;
        let stderr = &request.stderr;
        let (report, invocation_error) = match report {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        let out = String::from_utf8_lossy(&fs::read(stdout).unwrap_or_default()).into_owned();
        let err = String::from_utf8_lossy(&fs::read(stderr).unwrap_or_default()).into_owned();
        let passed = report.as_ref().is_some_and(|r| match expected {
            ExpectedExit::Success => qualifies(r, &err, None),
            ExpectedExit::Assertion(marker) => qualifies(r, &err, Some(marker)),
            ExpectedExit::Code(code) => {
                r.returncode == Some(code)
                    && r.cleanup_verified
                    && r.cleanup_failure.is_none()
                    && !r.timed_out
                    && !r.cancelled
                    && r.failure.as_deref() == Some(&format!("child exit Some({code})"))
            }
            ExpectedExit::StoppedOrSuccess => qualifies(r, &err, None) || r.stopped_cleanly(),
        });
        let marker = if let ExpectedExit::Assertion(marker) = expected {
            Some(marker)
        } else {
            None
        };
        let exit = if let ExpectedExit::Code(code) = expected {
            Some(code)
        } else {
            None
        };
        self.evidence["commands"].as_array_mut().unwrap().push(json!({
            "argv":request.argv,"cwd":request.cwd,"name":name,"exit_code":report.as_ref().and_then(|r| r.returncode),
            "timeout_seconds":request.timeout_seconds,"maximum_bytes":request.maximum_bytes,
            "supervision":report,"invocation_error":invocation_error,"stdout":stdout.file_name().and_then(|s|s.to_str()),
            "stderr":stderr.file_name().and_then(|s|s.to_str()),"expected_assertion":marker,"expected_exit":exit,"passed":passed
        }));
        self.save()?;
        if !passed {
            let diagnostic = report
                .as_ref()
                .and_then(|report| {
                    report
                        .cleanup_failure
                        .as_deref()
                        .or(report.failure.as_deref())
                })
                .or(invocation_error.as_deref())
                .unwrap_or("unexpected terminal status");
            anyhow::bail!(
                "{name} failed ({diagnostic}). See {}",
                self.output.display()
            );
        }
        Ok(out.trim_end_matches(['\r', '\n']).to_owned())
    }
    pub fn finish(mut self, result: Result<()>) -> Result<()> {
        let checked = result.and_then(|()| {
            ensure!(!self.cancellation.requested(), "campaign cancelled");
            ensure!(
                self.before == snapshot(&self.run.root)?,
                "source changed during diagnostic campaign"
            );
            ensure!(!self.cancellation.requested(), "campaign cancelled");
            Ok(())
        });
        self.evidence["passed"] = json!(checked.is_ok());
        self.evidence["cancelled"] = json!(self.cancellation.requested());
        if let Err(e) = &checked {
            if self.evidence.get("verified").is_some() {
                self.evidence["verified"] = json!(false);
            }
            self.evidence["error"] = json!(format!("{e:#}"));
            if self.evidence["status"].as_str().is_some_and(|s| {
                matches!(
                    s,
                    "incomplete"
                        | "measured"
                        | "calibrated"
                        | "completed-separated-reports"
                        | "qualified"
                )
            }) {
                self.evidence["status"] = json!("rejected");
            }
        }
        self.save()?;
        checked
    }
}

fn ready_line(output: &Path) -> Result<Option<String>> {
    let file = match fs::File::open(output) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    BufReader::new(file)
        .take(512)
        .read_until(b'\n', &mut bytes)?;
    ensure!(bytes.len() < 512, "oversized server readiness");
    if !bytes.ends_with(b"\n") {
        return Ok(None);
    }
    Ok(Some(
        std::str::from_utf8(&bytes)?
            .trim_end_matches(['\r', '\n'])
            .to_owned(),
    ))
}

fn loopback_address(line: &str) -> Result<SocketAddr> {
    let address: SocketAddr = line
        .strip_prefix("ready ")
        .context("server readiness missing")?
        .parse()?;
    ensure!(
        address.ip().is_loopback() && address.port() != 0,
        "server advertised an invalid loopback address"
    );
    Ok(address)
}
pub fn qualifies(report: &supervise::Report, stderr: &str, expected: Option<&str>) -> bool {
    // The shared supervisor reports an ordinary nonzero exit as a failure too.
    // Only its exact exit-only diagnostic can be accepted by a negative control;
    // deadline, output, observation and cleanup failures still reject.
    let clean_execution = report.failure.is_none()
        || (expected.is_some()
            && report.failure.as_deref()
                == Some(format!("child exit {:?}", report.returncode).as_str()));
    report.cleanup_verified
        && clean_execution
        && report.cleanup_failure.is_none()
        && !report.cancelled
        && !report.timed_out
        && match expected {
            None => report.returncode == Some(0),
            Some(marker) => {
                !marker.is_empty()
                    && report.returncode.is_some_and(|c| c != 0)
                    && stderr.contains(marker)
            }
        }
}
pub fn host(version: &str) -> Result<&str> {
    version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .filter(|s| !s.is_empty())
        .context("rustc host missing")
}
pub fn archive_source(c: &mut Campaign<'_>, reference: &str) -> Result<(String, PathBuf)> {
    let (revision, source, digest) = archive_revision(c, reference, None)?;
    c.evidence["engine_revision"] = json!(revision);
    c.evidence["archive_sha256"] = json!(digest);
    Ok((revision, source))
}

pub fn archive_revision(
    c: &mut Campaign<'_>,
    reference: &str,
    label: Option<&str>,
) -> Result<(String, PathBuf, String)> {
    let prefix = label.unwrap_or("engine");
    ensure!(
        !prefix.is_empty()
            && prefix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid archive label"
    );
    let revision = c.command(
        vec![
            "git".into(),
            "rev-parse".into(),
            "--verify".into(),
            "--end-of-options".into(),
            format!("{reference}^{{commit}}").into(),
        ],
        &format!("{prefix}-revision"),
        None,
        30.,
    )?;
    ensure!(
        revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid engine revision"
    );
    let files = c.command(
        cmd(["git", "ls-tree", "-r", &revision]),
        &format!("{prefix}-files"),
        None,
        30.,
    )?;
    ensure!(
        !files.is_empty()
            && files
                .lines()
                .all(|l| l.starts_with("100644 blob ") || l.starts_with("100755 blob ")),
        "nonregular engine archive input"
    );
    let (tar, source) = if let Some(label) = label {
        let parent = c.output.join("source");
        fs::create_dir_all(&parent)?;
        (c.output.join(format!("{label}.tar")), parent.join(label))
    } else {
        (c.output.join("source.tar"), c.output.join("source"))
    };
    fs::create_dir(&source)?;
    c.command(
        vec![
            "git".into(),
            "archive".into(),
            "--format=tar".into(),
            "--output".into(),
            path(&tar),
            revision.clone().into(),
        ],
        &format!("{prefix}-archive"),
        None,
        30.,
    )?;
    c.command(
        vec![
            "tar".into(),
            "-xf".into(),
            path(&tar),
            "-C".into(),
            path(&source),
        ],
        &if label.is_none() {
            "extract-source".to_owned()
        } else {
            format!("{prefix}-extract")
        },
        None,
        30.,
    )?;
    Ok((revision, source, sha(&tar)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_expected_exit_rejects_success_and_another_failure() {
        let tmp = crate::support::Scratch::new(&std::env::temp_dir(), "expected-exit").unwrap();
        let run = Run::new(tmp.path().to_owned());
        let mut c =
            Campaign::detached(&run, &tmp.path().join("output"), "command exit control").unwrap();
        c.command_exit(cmd(["sh", "-c", "exit 70"]), "expected", 70, 5.)
            .unwrap();
        for (name, expression) in [("success", "exit 0"), ("wrong", "exit 71")] {
            assert!(
                c.command_exit(cmd(["sh", "-c", expression]), name, 70, 5.)
                    .is_err()
            );
        }
        let observed = c.evidence["commands"].as_array().unwrap();
        assert_eq!(observed.len(), 3);
        for (record, expected) in observed.iter().zip([true, false, false]) {
            assert_eq!(record["expected_exit"], 70);
            assert_eq!(record["passed"], expected);
            assert_eq!(record["supervision"]["cleanup_verified"], true);
        }
    }
    #[test]
    fn loopback_ready_exit_stop_and_rejection_record_terminal_state() {
        let tmp =
            crate::support::Scratch::new(&std::env::temp_dir(), "loopback-supervision").unwrap();
        let run = Run::new(tmp.path().to_owned());
        for (name, script, natural, action_error, error) in [
            (
                "finite",
                "printf 'ready 127.0.0.1:51111\n'; while [ ! -f release ]; do sleep .01; done",
                true,
                false,
                None,
            ),
            (
                "stopped",
                "printf 'ready 127.0.0.1:51111\n'; exec sleep 30",
                false,
                false,
                None,
            ),
            (
                "bad-address",
                "printf 'ready 0.0.0.0:51111\n'; exec sleep 30",
                false,
                false,
                Some("invalid loopback address"),
            ),
            (
                "client-error",
                "printf 'ready 127.0.0.1:51111\n'; exec sleep 30",
                true,
                true,
                Some("client failed"),
            ),
            (
                "no-readiness",
                "exec sleep 30",
                false,
                false,
                Some("readiness deadline"),
            ),
        ] {
            let output = tmp.path().join(name);
            let mut campaign = Campaign::detached(&run, &output, "loopback control").unwrap();
            let readiness = if name == "no-readiness" {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(3)
            };
            let result = campaign.loopback(
                cmd(["sh", "-c", script]),
                "server",
                (readiness, 5.),
                natural,
                |_, address| {
                    ensure!(address.port() == 51111, "unexpected test address");
                    ensure!(!action_error, "client failed");
                    if natural {
                        fs::write(tmp.path().join("release"), [])?;
                    }
                    Ok(())
                },
            );
            let result = campaign.finish(result);
            let proof = crate::delivery::read_json(&output.join("evidence.json")).unwrap();
            let process = &proof["commands"][0]["supervision"];
            assert_eq!(process["cleanup_verified"], true, "{name}");
            assert_eq!(process["stopped"], name != "finite", "{name}");
            if let Some(marker) = error {
                assert!(result.unwrap_err().to_string().contains(marker), "{name}");
                assert_eq!(proof["passed"], false);
            } else {
                result.unwrap();
                assert_eq!(proof["passed"], true);
            }
        }
    }
    #[test]
    fn loopback_unwind_retires_live_server_without_waiting_for_deadline() {
        let tmp = crate::support::Scratch::new(&std::env::temp_dir(), "loopback-unwind").unwrap();
        let run = Run::new(tmp.path().to_owned());
        let output = tmp.path().join("output");
        let mut campaign = Campaign::detached(&run, &output, "loopback unwind control").unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = campaign.loopback(
                cmd([
                    "sh",
                    "-c",
                    "printf 'ready 127.0.0.1:51111\n'; exec sleep 30",
                ]),
                "server",
                (Duration::from_secs(3), 5.),
                false,
                |_, _| -> Result<()> { panic!("fixture action panicked") },
            );
        }));
        assert!(result.is_err());
        let report = crate::delivery::read_json(&output.join("server.process.json")).unwrap();
        assert_eq!(report["stopped"], true);
        assert_eq!(report["timed_out"], false);
        assert_eq!(report["cleanup_verified"], true);
    }
    #[test]
    fn paired_commands_overlap_and_both_reports_survive_failure() {
        let tmp =
            crate::support::Scratch::new(&std::env::temp_dir(), "paired-supervision").unwrap();
        let mut run = Run::new(tmp.path().to_owned());
        let journal = tmp.path().join("journal.jsonl");
        run.env
            .insert("HELPER_PROCESS_JOURNAL".into(), path(&journal));
        let output = tmp.path().join("output");
        let mut campaign = Campaign::detached(&run, &output, "parallel control").unwrap();
        let result = campaign.pair(
            [
                (
                    "first",
                    cmd([
                        "sh",
                        "-c",
                        "touch first; while [ ! -f second ]; do sleep .01; done; exit 7",
                    ]),
                ),
                (
                    "second",
                    cmd([
                        "sh",
                        "-c",
                        "while [ ! -f first ]; do sleep .01; done; touch second",
                    ]),
                ),
            ],
            5.,
        );
        assert!(campaign.finish(result).is_err());
        let proof = crate::delivery::read_json(&output.join("evidence.json")).unwrap();
        let reports = proof["commands"].as_array().unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0]["exit_code"], 7);
        assert_eq!(reports[1]["exit_code"], 0);
        assert!(
            reports
                .iter()
                .all(|row| row["supervision"]["cleanup_verified"] == true),
            "paired supervision: {reports:#?}"
        );
        let mut journal_pids = fs::read_to_string(journal)
            .unwrap()
            .lines()
            .map(|row| {
                serde_json::from_str::<supervise::Identity>(row)
                    .unwrap()
                    .pid as u64
            })
            .collect::<Vec<_>>();
        let mut report_pids = reports
            .iter()
            .map(|row| row["supervision"]["pid"].as_u64().unwrap())
            .collect::<Vec<_>>();
        journal_pids.sort_unstable();
        report_pids.sort_unstable();
        assert_eq!(journal_pids, report_pids);
    }
    fn fixture() -> (crate::support::Scratch, Run) {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "campaign").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/input.rs"), "before").unwrap();
        let git = dir.path().join("git");
        fs::write(&git,"#!/bin/sh\ncase \"$1\" in rev-parse) echo fixture-source;; status) exit 0;; *) exit 9;; esac\n").unwrap();
        fs::set_permissions(git, fs::Permissions::from_mode(0o700)).unwrap();
        let mut run = Run::new(dir.path().to_owned());
        let mut paths = vec![dir.path().to_owned()];
        paths.extend(std::env::split_paths(
            &run.env[&crate::support::path("PATH")],
        ));
        run.env.insert(
            crate::support::path("PATH"),
            std::env::join_paths(paths).unwrap(),
        );
        (dir, run)
    }
    #[test]
    fn cancellation_between_commands_stays_latched_through_retirement() {
        const MARKER: &str = "XTASK_CAMPAIGN_CANCELLATION_FIXTURE";
        if let Some(root) = std::env::var_os(MARKER) {
            let run = Run::new(PathBuf::from(root));
            let mut c =
                Campaign::detached(&run, Path::new("proof"), "cancellation control").unwrap();
            c.evidence["verified"] = json!(true);
            // SAFETY: this isolated test subprocess has the campaign handler
            // installed, with no active command handler to mask the lifetime gap.
            assert_eq!(unsafe { libc::raise(libc::SIGTERM) }, 0);
            assert!(
                c.command(cmd(["/bin/sh", "-c", "exit 99"]), "forbidden", None, 10.)
                    .is_err()
            );
            assert_eq!(
                c.retirement_command(cmd(["/bin/sh", "-c", "printf retired"]), "retirement")
                    .unwrap(),
                "retired"
            );
            assert!(c.finish(Ok(())).is_err());
            assert!(!run.root.join("proof/forbidden.stdout.log").exists());
            return;
        }
        let dir =
            crate::support::Scratch::new(&std::env::temp_dir(), "campaign-cancellation").unwrap();
        let env = std::env::vars()
            .chain([(MARKER.into(), dir.path().to_str().unwrap().into())])
            .collect();
        let r = supervise::run(&Request {
            argv: vec![
                std::env::current_exe().unwrap().to_str().unwrap().into(),
                "--exact".into(),
                "assurance::tests::cancellation_between_commands_stays_latched_through_retirement"
                    .into(),
                "--nocapture".into(),
            ],
            cwd: Some(dir.path().to_owned()),
            env: Some(env),
            stdout: dir.path().join("stdout"),
            stderr: dir.path().join("stderr"),
            report: None,
            // Includes instrumented self-binary hashing before SIGTERM; the
            // existing 15-second cancellation/retirement regressions stay intact.
            timeout_seconds: 60.,
            maximum_bytes: 1024 * 1024,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        })
        .unwrap();
        assert!(r.success(), "{r:?}");
        let proof: Value =
            serde_json::from_slice(&fs::read(dir.path().join("proof/evidence.json")).unwrap())
                .unwrap();
        assert_eq!(proof["cancelled"], true);
        assert_eq!(proof["passed"], false);
        assert_eq!(proof["verified"], false);
        assert_eq!(proof["commands"].as_array().unwrap().len(), 1);
        assert_eq!(
            proof["commands"][0]["supervision"]["cleanup_verified"],
            true
        );
    }
    #[test]
    fn configured_limits_reach_child_files_and_both_streams() {
        let (_dir, run) = fixture();
        let mut c = Campaign::new(&run, Path::new("limits"), "test").unwrap();
        assert!(c.limits(0, Some(1024)).is_err());
        assert!(c.limits(1024, Some(0)).is_err());
        c.limits(1024, Some(1024)).unwrap();
        let file = c.output.join("payload");
        assert!(
            c.command(
                vec![
                    "/bin/dd".into(),
                    "if=/dev/zero".into(),
                    format!("of={}", file.display()).into(),
                    "bs=2048".into(),
                    "count=1".into()
                ],
                "file",
                None,
                10.
            )
            .is_err()
        );
        assert_eq!(fs::metadata(file).unwrap().len(), 1024);
        for (label, redirect) in [("stdout", ""), ("stderr", " >&2")] {
            let script = format!(
                "while :; do printf 'abcdefghijklmnopqrstuvwxyz0123456789'{redirect}; done"
            );
            assert!(
                c.command(cmd(["/bin/sh", "-c", &script]), label, None, 10.)
                    .is_err()
            );
            let row = c.evidence["commands"].as_array().unwrap().last().unwrap();
            assert_eq!(row["supervision"]["cleanup_verified"], true);
            assert!(
                row["supervision"]["failure"]
                    .as_str()
                    .unwrap()
                    .contains("limit")
            );
        }
    }
    #[test]
    fn campaign_failures_persist_terminal_evidence_and_stdout_cannot_satisfy_marker() {
        let (_dir, run) = fixture();
        for (name, script, passed) in [
            ("valid", "echo required-marker >&2; exit 3", true),
            ("stdout", "echo required-marker; exit 3", false),
            ("missing", "echo wrong-marker >&2; exit 3", false),
            ("zero", "echo required-marker >&2; exit 0", false),
        ] {
            let mut c = Campaign::new(&run, Path::new(name), "test").unwrap();
            let result = c
                .command(
                    cmd(["/bin/sh", "-c", script]),
                    "control",
                    Some("required-marker"),
                    5.,
                )
                .map(|_| ());
            assert_eq!(result.is_ok(), passed);
            let output = c.output.clone();
            assert_eq!(c.finish(result).is_ok(), passed);
            let evidence: Value =
                serde_json::from_slice(&fs::read(output.join("evidence.json")).unwrap()).unwrap();
            assert_eq!(evidence["passed"], passed);
            assert_eq!(
                evidence["commands"][2]["supervision"]["cleanup_verified"],
                true
            );
        }
    }
    #[test]
    fn campaign_source_drift_and_timeout_never_qualify() {
        let (_dir, run) = fixture();
        let mut c = Campaign::new(&run, Path::new("drift"), "test").unwrap();
        c.evidence["status"] = json!("measured");
        c.evidence["verified"] = json!(true);
        fs::write(run.root.join("src/input.rs"), "after").unwrap();
        let output = c.output.clone();
        assert!(c.finish(Ok(())).is_err());
        let evidence: Value =
            serde_json::from_slice(&fs::read(output.join("evidence.json")).unwrap()).unwrap();
        assert_eq!(evidence["passed"], false);
        assert_eq!(evidence["verified"], false);
        assert_eq!(evidence["status"], "rejected");
        assert!(
            evidence["error"]
                .as_str()
                .unwrap()
                .contains("source changed")
        );
        let mut c = Campaign::new(&run, Path::new("timeout"), "test").unwrap();
        let result = c
            .command(
                cmd(["/bin/sh", "-c", "echo required-marker >&2; exec sleep 30"]),
                "control",
                Some("required-marker"),
                0.05,
            )
            .map(|_| ());
        let output = c.output.clone();
        assert!(c.finish(result).is_err());
        let evidence: Value =
            serde_json::from_slice(&fs::read(output.join("evidence.json")).unwrap()).unwrap();
        assert_eq!(evidence["passed"], false);
        assert_eq!(evidence["commands"][2]["supervision"]["timed_out"], true);
        assert_eq!(
            evidence["commands"][2]["supervision"]["cleanup_verified"],
            true
        );
    }
    #[test]
    fn expected_failure_needs_stderr_marker_and_successful_retirement() {
        let mut r = supervise::Report {
            returncode: Some(1),
            cleanup_verified: true,
            ..Default::default()
        };
        assert!(qualifies(&r, "specific failure", Some("specific")));
        assert!(!qualifies(&r, "wrong failure", Some("specific")));
        assert!(!qualifies(&r, "specific failure", None));
        r.returncode = Some(0);
        assert!(!qualifies(&r, "specific failure", Some("specific")));
        assert!(qualifies(&r, "", None));
        r.returncode = Some(1);
        r.cleanup_verified = false;
        assert!(!qualifies(&r, "specific failure", Some("specific")));
        r.cleanup_verified = true;
        r.timed_out = true;
        assert!(!qualifies(&r, "specific failure", Some("specific")));
        r.timed_out = false;
        r.failure = Some("output limit".into());
        assert!(!qualifies(&r, "specific failure", Some("specific")));
    }
    #[test]
    fn source_changes_are_visible_and_corpora_excluded() {
        let dir = crate::support::Scratch::new(&std::env::temp_dir(), "source-inputs").unwrap();
        for name in [
            "src/engine.rs",
            "fuzz/src/peer.rs",
            "fuzz/target/generated.rs",
            "fuzz/corpus/input.rs",
            "apps/peer/src/parser.rs",
        ] {
            let p = dir.path().join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "before").unwrap();
        }
        let before = snapshot(dir.path()).unwrap();
        assert_eq!(
            before.keys().cloned().collect::<Vec<_>>(),
            [
                "apps/peer/src/parser.rs",
                "fuzz/src/peer.rs",
                "src/engine.rs"
            ]
        );
        fs::write(dir.path().join("fuzz/corpus/input.rs"), "after").unwrap();
        assert_eq!(snapshot(dir.path()).unwrap(), before);
        fs::write(dir.path().join("apps/peer/src/parser.rs"), "after").unwrap();
        assert_ne!(snapshot(dir.path()).unwrap(), before);
    }
}
