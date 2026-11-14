mod check;
mod clean;
#[cfg(unix)]
mod faults;
mod package;
#[cfg(unix)]
mod supervise;
mod support;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::{env, ffi::OsString, path::PathBuf};
use support::{Platform, Run, cmd, path};

#[derive(Parser)]
#[command(
    name = "xtask",
    version,
    about = "Private repository automation; never publishes crates",
    after_help = "Set WEEKEND_PYTHON for the retained Python leaf checkers. The runner cache lives in target/xtask."
)]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Run bounded native or exact-delivery fault qualifications; never publishes.
    #[cfg(unix)]
    Faults {
        #[command(subcommand)]
        task: faults::Task,
    },
    /// Execute one JSON request with bounded POSIX supervision (private protocol).
    #[cfg(unix)]
    Supervise,
    #[command(hide = true)]
    #[cfg(unix)]
    ProcessTable,
    #[command(hide = true)]
    #[cfg(unix)]
    RunnerSeed {
        #[arg(long)]
        output: PathBuf,
    },
    /// Run the existing debug/release, CLI, renamed consumer and native ABI checks.
    Check {
        #[arg(long)]
        skip_odin: bool,
        #[arg(short = 'n', long)]
        dry_run: bool,
    },
    /// Build the workspace offline with its lockfile; honors CARGO_TARGET_DIR.
    Build(Build),
    /// Build and qualify a private exact-artifact package; never publishes it.
    Package(package::Options),
    /// Remove normal/nested Cargo caches; keeps saved proofs and target/xtask.
    Clean {
        /// Also remove saved root-target qualification outputs; keeps target/xtask.
        #[arg(long)]
        proofs: bool,
        #[arg(short = 'n', long)]
        dry_run: bool,
    },
}

#[derive(Args, Debug)]
struct Build {
    #[arg(short = 'r', long)]
    release: bool,
    #[arg(
        short = 'F',
        long,
        value_name = "LIST",
        conflicts_with = "all_features"
    )]
    features: Option<String>,
    #[arg(long)]
    all_features: bool,
    #[arg(long, value_name = "TRIPLE")]
    target: Option<String>,
}
impl Build {
    fn command(self) -> Vec<OsString> {
        let mut out = cmd(["cargo", "build", "--offline", "--locked", "--workspace"]);
        if self.release {
            out.push(path("--release"));
        }
        if let Some(features) = self.features {
            out.extend([path("--features"), path(features)]);
        }
        if self.all_features {
            out.push(path("--all-features"));
        }
        if let Some(target) = self.target {
            out.extend([path("--target"), path(target)]);
        }
        out
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = execute(cli) {
        eprintln!("xtask: {error:#}");
        std::process::exit(1);
    }
}

fn execute(cli: Cli) -> Result<()> {
    let root = dunce::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap())?;
    let mut run = Run::new(root.clone());
    match cli.task {
        #[cfg(unix)]
        Task::Faults { task } => faults::run(&run, task),
        #[cfg(unix)]
        Task::Supervise => supervise::cli(),
        #[cfg(unix)]
        Task::ProcessTable => supervise::table_cli(),
        #[cfg(unix)]
        Task::RunnerSeed { output } => faults::stage_runner(&root, &output),
        Task::Check { skip_odin, dry_run } => {
            check::run(&mut run, Platform::host()?, skip_odin, dry_run)
        }
        Task::Build(build) => run.status(&build.command()),
        Task::Package(options) => package::run(&mut run, options),
        Task::Clean { proofs, dry_run } => {
            let tracked = run.capture(&cmd(["git", "ls-files", "-z"]))?;
            clean::run(
                &root,
                &tracked,
                proofs,
                dry_run,
                &dunce::canonicalize(env::current_exe()?)?,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn build_rejects_ambiguous_or_extra_commands() {
        for args in [
            vec!["--features"],
            vec!["--release", "--release"],
            vec!["--features", "x", "--all-features"],
            vec!["publish"],
            vec!["--target", "--release"],
        ] {
            assert!(Cli::try_parse_from(["xtask", "build"].into_iter().chain(args)).is_err());
        }
        let cli = Cli::try_parse_from([
            "xtask",
            "build",
            "--release",
            "--features",
            "crdt,replication",
        ])
        .unwrap();
        let Task::Build(build) = cli.task else {
            panic!("build task")
        };
        assert_eq!(
            build.command(),
            cmd([
                "cargo",
                "build",
                "--offline",
                "--locked",
                "--workspace",
                "--release",
                "--features",
                "crdt,replication"
            ])
        );
    }
    #[test]
    fn command_help_and_constraints_are_generated_by_clap() {
        use clap::{CommandFactory, error::ErrorKind};
        Cli::command().debug_assert();
        for task in ["check", "build", "package", "clean"] {
            assert_eq!(
                Cli::try_parse_from(["xtask", task, "--help"])
                    .err()
                    .unwrap()
                    .kind(),
                ErrorKind::DisplayHelp
            );
        }
        assert!(Cli::try_parse_from(["xtask", "publish"]).is_err());
    }
}
