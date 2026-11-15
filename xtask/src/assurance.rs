//! Source-bound diagnostic campaigns. Expected failures still require verified
//! process retirement and a specific stderr diagnostic, never just a nonzero exit.
use crate::{
    supervise::{self, Request},
    support::{Run, cmd, sha},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
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
pub struct Campaign<'a> {
    pub run: &'a Run,
    pub output: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub evidence: Value,
    before: BTreeMap<String, String>,
}
impl<'a> Campaign<'a> {
    pub fn new(run: &'a Run, output: &Path, scope: &str) -> Result<Self> {
        let output = fresh(&run.root, output)?;
        let before = snapshot(&run.root)?;
        let mut campaign = Self {
            run,
            output,
            env: run.env.clone(),
            before: before.clone(),
            evidence: json!({"passed":false,"scope":scope,"source_hashes":before,"commands":[]}),
        };
        campaign.save()?;
        let source = campaign.command(cmd(["git", "rev-parse", "HEAD"]), "source", None, 30.)?;
        campaign.evidence["source"] = json!(source.trim());
        let dirty = campaign.command(cmd(["git", "status", "--porcelain"]), "dirty", None, 30.)?;
        campaign.evidence["dirty"] = json!(!dirty.trim().is_empty());
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
    pub fn unchanged(&self) -> Result<bool> {
        Ok(self.before == snapshot(&self.run.root)?)
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
        let report = supervise::run(&Request {
            argv: strings.clone(),
            cwd: Some(cwd.to_owned()),
            env: Some(env),
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            report: Some(self.output.join(format!("{name}.process.json"))),
            timeout_seconds: timeout,
            maximum_bytes: 64 * 1024 * 1024,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        });
        let (report, invocation_error) = match report {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        let out = String::from_utf8_lossy(&fs::read(&stdout).unwrap_or_default()).into_owned();
        let err = String::from_utf8_lossy(&fs::read(&stderr).unwrap_or_default()).into_owned();
        let passed = report
            .as_ref()
            .is_some_and(|r| qualifies(r, &err, expected));
        self.evidence["commands"].as_array_mut().unwrap().push(json!({"argv":strings,"cwd":cwd,"name":name,"exit_code":report.as_ref().and_then(|r| r.returncode),"supervision":report,"invocation_error":invocation_error,"stdout":stdout.file_name().and_then(|s|s.to_str()),"stderr":stderr.file_name().and_then(|s|s.to_str()),"expected_assertion":expected,"passed":passed}));
        self.save()?;
        ensure!(passed, "{name} failed; see {}", self.output.display());
        Ok(out.trim_end_matches(['\r', '\n']).to_owned())
    }
    pub fn finish(mut self, result: Result<()>) -> Result<()> {
        let checked = result.and_then(|()| {
            ensure!(
                self.before == snapshot(&self.run.root)?,
                "source changed during diagnostic campaign"
            );
            Ok(())
        });
        self.evidence["passed"] = json!(checked.is_ok());
        if let Err(e) = &checked {
            self.evidence["error"] = json!(format!("{e:#}"));
            if self.evidence["status"].as_str().is_some_and(|s| {
                matches!(
                    s,
                    "incomplete" | "measured" | "calibrated" | "completed-separated-reports"
                )
            }) {
                self.evidence["status"] = json!("rejected");
            }
        }
        self.save()?;
        checked
    }
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
#[cfg(test)]
mod tests {
    use super::*;
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
        fs::write(run.root.join("src/input.rs"), "after").unwrap();
        let output = c.output.clone();
        assert!(c.finish(Ok(())).is_err());
        let evidence: Value =
            serde_json::from_slice(&fs::read(output.join("evidence.json")).unwrap()).unwrap();
        assert_eq!(evidence["passed"], false);
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
