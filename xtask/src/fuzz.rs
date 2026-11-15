use crate::{
    assurance::{self, Campaign},
    support::{Run, cmd, path, sha},
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use regex::Regex;
use serde_json::json;
use std::{fs, path::PathBuf};
#[derive(Debug, Args)]
pub struct Options {
    #[arg(long = "target")]
    pub targets: Vec<String>,
    #[arg(long,default_value_t=1000, value_parser=clap::value_parser!(u64).range(1..))]
    pub runs: u64,
    #[arg(long,default_value_t=60, value_parser=clap::value_parser!(u64).range(1..=85800))]
    pub seconds: u64,
    #[arg(long,default_value_t=2048, value_parser=clap::value_parser!(u64).range(1..))]
    pub rss_mb: u64,
    #[arg(long,default_value_t=262144, value_parser=clap::value_parser!(u64).range(1..))]
    pub max_len: u64,
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    #[arg(long, default_value = "nightly")]
    pub toolchain: String,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, default_value = "target/fuzz-build")]
    pub build_directory: PathBuf,
}
fn targets(manifest: &str) -> Result<Vec<String>> {
    let manifest: toml::Table = toml::from_str(manifest)?;
    let names = manifest
        .get("bin")
        .and_then(toml::Value::as_array)
        .context("fuzz bins missing")?
        .iter()
        .map(|row| {
            let name = row
                .get("name")
                .and_then(toml::Value::as_str)
                .context("fuzz bin name")?;
            ensure!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
                "unsafe fuzz target name"
            );
            Ok(name.to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(!names.is_empty(), "empty fuzz target inventory");
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    ensure!(unique.len() == names.len(), "duplicate fuzz targets");
    Ok(names)
}
fn completed_runs(log: &str) -> Option<u64> {
    Regex::new(r"(?m)^Done (\d+) runs in \d+ second\(s\)\r?$")
        .unwrap()
        .captures_iter(log)
        .last()
        .and_then(|m| m[1].parse().ok())
}
pub fn run(run: &Run, options: Options) -> Result<()> {
    let available = targets(&fs::read_to_string(run.root.join("fuzz/Cargo.toml"))?)?;
    let selected = if options.targets.is_empty() {
        available.clone()
    } else {
        options.targets
    };
    let unique: std::collections::BTreeSet<_> = selected.iter().collect();
    ensure!(
        unique.len() == selected.len() && selected.iter().all(|name| available.contains(name)),
        "unknown or duplicate target"
    );
    let output = options.output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "target/fuzz-evidence/{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    });
    let mut c = Campaign::new(
        run,
        &output,
        "Bounded ASan fuzz execution; report completed executions, not requested budgets",
    )?;
    let result = (|| {
        let before_seeds = assurance::seeds(&run.root)?;
        let before_lock = sha(&run.root.join("fuzz/Cargo.lock"))?;
        let toolchain = format!("+{}", options.toolchain);
        let version = c.command(
            cmd(["rustc", &toolchain, "-vV"]),
            "rustc-version",
            None,
            30.,
        )?;
        let fuzz = c.command(
            cmd(["cargo", "fuzz", "--version"]),
            "cargo-fuzz-version",
            None,
            30.,
        )?;
        c.evidence["toolchain"] = json!(version);
        c.evidence["cargo_fuzz"] = json!(fuzz);
        c.evidence["seed_sha256"] = json!(before_seeds);
        c.evidence["lockfile_sha256"] = json!(before_lock);
        c.evidence["targets"] = json!([]);
        c.save()?;
        let mut failed = false;
        for name in selected {
            let directory = c.output.join(&name);
            let corpus = directory.join("corpus");
            let artifacts = directory.join("artifacts");
            fs::create_dir(&directory)?;
            fs::create_dir(&corpus)?;
            fs::create_dir(&artifacts)?;
            let mut command = cmd([
                "cargo",
                &toolchain,
                "fuzz",
                "run",
                "--sanitizer",
                "address",
                "--target-dir",
            ]);
            command.extend([
                path(assurance::absolute(&run.root, &options.build_directory)),
                path(&name),
                path(&corpus),
            ]);
            let seeds = run.root.join("fuzz/seeds").join(&name);
            if seeds.is_dir() {
                command.push(path(seeds));
            }
            command.push(path("--"));
            command.extend([
                path(format!("-runs={}", options.runs)),
                path(format!("-max_total_time={}", options.seconds)),
                path(format!("-rss_limit_mb={}", options.rss_mb)),
                path(format!("-max_len={}", options.max_len)),
                path(format!("-seed={}", options.seed)),
                path("-timeout=10"),
                path(format!("-artifact_prefix={}/", artifacts.display())),
            ]);
            let label = format!("target-{name}");
            let outcome = c.command(command, &label, None, (options.seconds + 600) as f64);
            let mut log =
                fs::read(c.output.join(format!("{label}.stdout.log"))).unwrap_or_default();
            log.extend(fs::read(c.output.join(format!("{label}.stderr.log"))).unwrap_or_default());
            fs::write(directory.join("run.log"), &log)?;
            let log = String::from_utf8_lossy(&log);
            let actual = completed_runs(&log);
            let source_changed = !c.unchanged()?;
            let seeds_changed = assurance::seeds(&run.root)? != before_seeds;
            let lock_changed = sha(&run.root.join("fuzz/Cargo.lock"))? != before_lock;
            let passed = outcome.is_ok()
                && actual.is_some_and(|n| n > 0)
                && !source_changed
                && !seeds_changed
                && !lock_changed;
            let command = c.evidence["commands"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .clone();
            c.evidence["targets"].as_array_mut().unwrap().push(json!({"name":name,"command":command["argv"],"status":command["exit_code"],"elapsed_seconds":command["supervision"]["elapsed_seconds"],"completed_runs":actual,"passed":passed,"source_changed":source_changed,"seeds_changed":seeds_changed,"lockfile_changed":lock_changed}));
            c.save()?;
            eprintln!(
                "{name}: completed {actual:?}, passed {passed}; evidence {}",
                c.output.display()
            );
            failed |= !passed;
        }
        ensure!(
            !failed,
            "one or more fuzz targets failed or completed no executions"
        );
        Ok(())
    })();
    c.finish(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requested_runs_and_failed_builds_are_not_completed_runs() {
        assert_eq!(
            completed_runs("cargo fuzz -runs=1000\nerror: failed to compile\n"),
            None
        );
        assert_eq!(completed_runs("Done 1000 runs in fake second(s)\n"), None);
        assert_eq!(completed_runs("Done 0 runs in 0 second(s)\n"), Some(0));
        assert_eq!(
            completed_runs("#538 DONE\nDone 538 runs in 21 second(s)\n"),
            Some(538)
        );
        assert_eq!(
            completed_runs("Done 1 runs in 0 second(s)\r\nDone 2 runs in 1 second(s)\r\n"),
            Some(2)
        );
        assert_eq!(
            completed_runs("Done 18446744073709551616 runs in 1 second(s)\n"),
            None
        );
    }
    #[test]
    fn target_inventory_rejects_empty_duplicate_or_unsafe_bins() {
        assert_eq!(
            targets("[[bin]]\nname='valid_target'\n").unwrap(),
            ["valid_target"]
        );
        for text in [
            "",
            "[[bin]]\nname='../escape'",
            "[[bin]]\nname='a'\n[[bin]]\nname='a'",
            "[[bin]]\nname=''",
            "[[bin]]\nname='.'",
        ] {
            assert!(targets(text).is_err(), "{text}");
        }
    }
}
