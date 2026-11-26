//! Bounded process boundary for portable controls and native Windows delivery.
use crate::support::sha;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, fs, path::Path};

#[derive(Default, Debug, Serialize)]
pub struct Report {
    pub pid: Option<u32>,
    pub exit: Option<i64>,
    pub seconds: f64,
    pub cleanup_verified: bool,
    pub failure: Option<String>,
    #[cfg(windows)]
    pub creation_time_100ns: Option<u64>,
    #[cfg(windows)]
    pub stopped: bool,
    #[cfg(windows)]
    pub timed_out: bool,
    #[cfg(windows)]
    pub output_exceeded: bool,
}
impl Report {
    #[cfg(windows)]
    pub fn stopped_cleanly(&self) -> bool {
        self.stopped
            && self.exit == Some(i64::from(crate::windows_process::JOB_TERMINATION_EXIT))
            && self.cleanup_verified
            && self.failure.is_none()
    }

    pub fn qualifies(&self, any_exit: bool) -> bool {
        #[cfg(windows)]
        if self.stopped {
            return false;
        }
        self.exit.is_some_and(|n| any_exit || n == 0)
            && self.cleanup_verified
            && self.failure.is_none()
    }
}
pub struct Request<'a> {
    pub argv: &'a [OsString],
    pub cwd: &'a Path,
    pub env: &'a BTreeMap<OsString, OsString>,
    pub stdout: &'a Path,
    pub stderr: &'a Path,
    pub timeout: f64,
    pub maximum: u64,
}
fn admit(request: &Request<'_>) -> Result<()> {
    ensure!(
        !request.argv.is_empty()
            && request.timeout.is_finite()
            && request.timeout > 0.
            && request.timeout <= 600.
            && request.maximum > 0
            && request.maximum <= 64 * 1024 * 1024,
        "invalid trace process request"
    );
    // Admit both destinations before creating either or launching anything.
    for p in [request.stdout, request.stderr] {
        ensure!(
            fs::symlink_metadata(p).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "trace log output already exists or cannot be inspected"
        );
    }
    ensure!(
        request.stdout != request.stderr,
        "separate trace streams required"
    );
    Ok(())
}

#[cfg(windows)]
pub fn run_stoppable(
    request: &Request<'_>,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<Report> {
    admit(request)?;
    crate::windows_process::run(request, Some(stop))
}

pub fn run(request: &Request<'_>) -> Result<Report> {
    admit(request)?;
    #[cfg(windows)]
    {
        crate::windows_process::run(request, None)
    }
    #[cfg(unix)]
    {
        let report = crate::supervise::run(&crate::supervise::Request {
            argv: request
                .argv
                .iter()
                .map(|s| s.to_str().map(str::to_owned).context("UTF-8 trace command"))
                .collect::<Result<_>>()?,
            cwd: Some(request.cwd.to_owned()),
            env: Some(
                request
                    .env
                    .iter()
                    .map(|(k, v)| {
                        Ok((
                            k.to_str().context("UTF-8 env name")?.into(),
                            v.to_str().context("UTF-8 env value")?.into(),
                        ))
                    })
                    .collect::<Result<_>>()?,
            ),
            stdout: request.stdout.to_owned(),
            stderr: request.stderr.to_owned(),
            report: None,
            timeout_seconds: request.timeout,
            maximum_bytes: request.maximum,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        })?;
        let exit = report.returncode.map(i64::from);
        let ordinary =
            report.failure.as_deref() == Some(&format!("child exit {:?}", report.returncode));
        Ok(Report {
            pid: report.pid,
            exit,
            seconds: report.elapsed_seconds,
            cleanup_verified: report.cleanup_verified && report.cleanup_failure.is_none(),
            failure: if report.cancelled || report.timed_out {
                Some("trace command cancelled/deadline".into())
            } else if ordinary {
                None
            } else {
                report.failure
            },
        })
    }
}
pub fn execute(
    argv: Vec<OsString>,
    cwd: &Path,
    label: &str,
    env: &BTreeMap<OsString, OsString>,
    commands: &mut Vec<Value>,
    any_exit: bool,
) -> Result<Report> {
    ensure!(
        !label.is_empty()
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "invalid trace log label"
    );
    let stdout = cwd.join(format!("{label}.stdout"));
    let stderr = cwd.join(format!("{label}.stderr"));
    let result = run(&Request {
        argv: &argv,
        cwd,
        env,
        stdout: &stdout,
        stderr: &stderr,
        timeout: 120.,
        maximum: 4 * 1024 * 1024,
    });
    let strings: Vec<_> = argv
        .iter()
        .map(|s| s.to_str().context("UTF-8 recorded trace command"))
        .collect::<Result<_>>()?;
    let row = match &result {
        Ok(report) => {
            json!({"command":strings,"report":report,"stdout_sha256":sha(&stdout).ok(),"stderr_sha256":sha(&stderr).ok()})
        }
        Err(e) => json!({"command":strings,"error":format!("{e:#}")}),
    };
    commands.push(row);
    let report = result?;
    ensure!(
        report.qualifies(any_exit),
        "trace command failed: {label}: {report:?}"
    );
    Ok(report)
}

pub fn admission(root: &Path, output: &Path) -> Result<std::path::PathBuf> {
    let output = root.join(output);
    match fs::symlink_metadata(&output) {
        Ok(_) => anyhow::bail!("output already exists"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    crate::notices::resolve(&output)
}

#[cfg(any(windows, target_os = "macos"))]
pub fn write_proof(root: &Path, proof: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(proof)?;
    bytes.push(b'\n');
    fs::write(root.join("evidence.json"), bytes)?;
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
pub fn call(
    proof: &mut Value,
    output: &Path,
    argv: Vec<OsString>,
    cwd: &Path,
    label: &str,
    env: &BTreeMap<OsString, OsString>,
    any_exit: bool,
) -> Result<Report> {
    let result = execute(
        argv,
        cwd,
        label,
        env,
        proof["commands"].as_array_mut().unwrap(),
        any_exit,
    );
    write_proof(output, proof)?;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    fn fixture(kind: &str) -> Vec<OsString> {
        #[cfg(unix)]
        {
            crate::support::cmd([
                "/bin/sh",
                "-c",
                match kind {
                    "ok" => "printf 'ok\\n'",
                    "bad" => "exit 2",
                    "overflow" => "while :; do printf abcdefghijklmnopqrstuvwxyz; done",
                    _ => "exec sleep 30",
                },
            ])
        }
        #[cfg(windows)]
        {
            crate::support::cmd([
                "cmd.exe",
                "/D",
                "/Q",
                "/C",
                match kind {
                    "ok" => "echo ok",
                    "bad" => "exit /b 2",
                    "overflow" => "for /L %i in (1,1,1000000) do @echo abcdefghijklmnopqrstuvwxyz",
                    _ => "ping -n 30 127.0.0.1 >nul",
                },
            ])
        }
    }
    fn call(root: &Path, label: &str, timeout: f64, maximum: u64) -> Report {
        let argv = fixture(label);
        let env = std::env::vars_os().collect();
        run(&Request {
            argv: &argv,
            cwd: root,
            env: &env,
            stdout: &root.join(format!("{label}.stdout")),
            stderr: &root.join(format!("{label}.stderr")),
            timeout,
            maximum,
        })
        .unwrap()
    }

    #[test]
    fn trace_qualification_requires_an_exit_and_verified_cleanup() {
        for (exit, any_exit, accepted) in [
            (Some(0), false, true),
            (Some(2), false, false),
            (Some(2), true, true),
            (None, true, false),
        ] {
            let report = Report {
                exit,
                cleanup_verified: true,
                ..Report::default()
            };
            assert_eq!(
                report.qualifies(any_exit),
                accepted,
                "exit {exit:?} with any_exit={any_exit}"
            );
        }
        let unverified = Report {
            exit: Some(0),
            ..Report::default()
        };
        assert!(!unverified.qualifies(false), "unverified cleanup qualified");
        let failed = Report {
            exit: Some(0),
            cleanup_verified: true,
            failure: Some("fixture".into()),
            ..Report::default()
        };
        assert!(!failed.qualifies(false), "a failed trace qualified");
    }

    #[test]
    fn real_success_and_nonzero_status_preserve_cleanup() {
        let t = Scratch::new(&std::env::temp_dir(), "trace-process-status").unwrap();
        let ok = call(t.path(), "ok", 10., 1024);
        assert!(ok.qualifies(false), "{ok:?}");
        assert_eq!(
            fs::read_to_string(t.path().join("ok.stdout"))
                .unwrap()
                .trim(),
            "ok"
        );
        let bad = call(t.path(), "bad", 10., 1024);
        assert_eq!(bad.exit, Some(2));
        assert!(!bad.qualifies(false));
        assert!(bad.qualifies(true));
        let argv = fixture("ok");
        let env = std::env::vars_os().collect();
        let mut commands = vec![];
        assert!(
            execute(argv, t.path(), "recorded", &env, &mut commands, false)
                .unwrap()
                .qualifies(false)
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["stdout_sha256"].as_str().unwrap().len(), 64);
        assert!(
            execute(
                fixture("bad"),
                t.path(),
                "rejected",
                &env,
                &mut commands,
                false
            )
            .is_err()
        );
        assert_eq!(commands.last().unwrap()["report"]["exit"], 2);
        assert!(
            execute(
                fixture("bad"),
                t.path(),
                "allowed",
                &env,
                &mut commands,
                true
            )
            .unwrap()
            .qualifies(true)
        );
    }
    #[test]
    fn output_overflow_and_deadline_never_qualify_as_expected_failures() {
        let t = Scratch::new(&std::env::temp_dir(), "trace-process-limits").unwrap();
        let overflow = call(t.path(), "overflow", 10., 128);
        assert!(!overflow.qualifies(true));
        assert!(overflow.cleanup_verified, "{overflow:?}");
        assert!(overflow.exit.is_some());
        assert!(
            overflow
                .failure
                .as_deref()
                .is_some_and(|s| s.contains("output limit")),
            "{overflow:?}"
        );
        assert!(
            fs::metadata(t.path().join("overflow.stdout"))
                .unwrap()
                .len()
                <= 128
        );
        let timeout = call(t.path(), "timeout", 0.2, 1024);
        assert!(!timeout.qualifies(true));
        assert!(timeout.cleanup_verified, "{timeout:?}");
        assert!(timeout.exit.is_some());
        assert!(
            timeout
                .failure
                .as_deref()
                .is_some_and(|s| s.contains("deadline")),
            "{timeout:?}"
        );
    }
}
