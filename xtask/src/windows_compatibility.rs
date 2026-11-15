//! Native process adapter for the frozen typed rollout oracle.
use crate::{
    support::{Run, cmd, sha},
    trace_process::{self, Request},
    windows_process,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

const INPUTS: &[&str] = &[
    "Cargo.lock",
    "xtask/Cargo.toml",
    "xtask/src/compatibility.rs",
    "xtask/src/source_archive.rs",
    "xtask/src/support.rs",
    "xtask/src/trace_process.rs",
    "xtask/src/windows_compatibility.rs",
    "xtask/src/windows_process.rs",
    "tools/compatibility-typed-v1.rs",
    "tools/compatibility-typed-v2.rs",
];

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
}

impl<'a> Campaign<'a> {
    pub fn new(run: &'a Run, output: &Path, scope: &str) -> Result<Self> {
        let output = trace_process::admission(&run.root, output)?;
        fs::create_dir_all(output.parent().context("output parent")?)?;
        fs::create_dir(&output).context("new evidence directory")?;
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
        ensure!(
            expected.is_none(),
            "typed qualification checks exact exits and stdout"
        );
        self.execute(args, name, 0, timeout, cwd)
    }

    pub fn command_exit(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        code: i32,
        timeout: f64,
    ) -> Result<String> {
        self.execute(args, name, i64::from(code), timeout, &self.run.root)
    }

    fn execute(
        &mut self,
        args: Vec<OsString>,
        name: &str,
        code: i64,
        timeout: f64,
        cwd: &Path,
    ) -> Result<String> {
        ensure!(
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid compatibility log label"
        );
        let argv = args
            .iter()
            .map(|arg| arg.to_str().context("UTF-8 recorded command"))
            .collect::<Result<Vec<_>>>()?;
        let stdout = self.output.join(format!("{name}.stdout"));
        let stderr = self.output.join(format!("{name}.stderr"));
        let result = trace_process::run(&Request {
            argv: &args,
            cwd,
            env: &self.env,
            stdout: &stdout,
            stderr: &stderr,
            timeout,
            maximum: 8 * 1024 * 1024,
        });
        let passed = result
            .as_ref()
            .is_ok_and(|r| r.qualifies(true) && r.exit == Some(code));
        let mut record = json!({
            "label":name,"argv":argv,"cwd":cwd,"expected_exit_code":code,"passed":passed,
            "stdout_sha256":sha(&stdout).ok(),"stderr_sha256":sha(&stderr).ok(),
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
        self.evidence["commands"]
            .as_array_mut()
            .unwrap()
            .push(record);
        self.save()?;
        let report = result?;
        ensure!(
            passed,
            "compatibility command {name} expected exit {code}: {report:?}"
        );
        Ok(fs::read_to_string(stdout)?.trim().into())
    }

    pub fn finish(mut self, result: Result<()>) -> Result<()> {
        let result = result.and_then(|()| {
            ensure!(
                self.evidence["source_hashes"] == json!(input_hashes(&self.run.root)?),
                "typed qualification inputs changed"
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
