mod abi;
#[cfg(unix)]
mod artifact_trace;
#[cfg(unix)]
mod assurance;
#[cfg(unix)]
mod atomic_model;
#[cfg(unix)]
mod benchmark;
#[cfg(unix)]
mod c_sanitizers;
mod check;
mod clean;
#[cfg(unix)]
mod compatibility;
#[cfg(unix)]
mod coverage;
#[cfg(unix)]
mod coverage_executables;
#[cfg(unix)]
mod delivered_faults;
mod delivery;
#[cfg(unix)]
mod faults;
#[cfg(unix)]
mod follower_backup;
#[cfg(unix)]
mod fuzz;
#[cfg(any(unix, test))]
mod instrumentation;
mod migration;
#[cfg(unix)]
mod monthly_workload;
#[cfg(unix)]
mod mutations;
mod notices;
mod odin;
mod package;
#[cfg(unix)]
mod probe;
#[cfg(unix)]
mod quota_enospc;
#[cfg(target_os = "linux")]
mod quota_guest;
#[cfg(target_os = "linux")]
mod quota_host;
#[cfg(target_os = "linux")]
mod quota_oracle;
#[cfg(unix)]
mod replica_progress;
#[cfg(unix)]
mod resource_campaign;
#[cfg(unix)]
mod resource_oracle;
#[cfg(unix)]
mod resource_probe;
mod rust_source;
#[cfg(unix)]
mod sanitizers;
#[cfg(unix)]
mod serialization_scale;
#[cfg(unix)]
mod snapshot_upgrade;
#[cfg(unix)]
mod soak;
#[cfg(unix)]
mod sqlite_baseline;
#[cfg(unix)]
mod supervise;
mod support;
mod toolchain;
mod trace_model;
#[cfg(any(windows, test))]
mod trace_process;
#[cfg(windows)]
mod windows_process;
#[cfg(any(windows, test))]
mod windows_trace;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::{env, ffi::OsString, path::PathBuf};
use support::{Platform, Run, cmd, path};

#[derive(Parser)]
#[command(
    name = "xtask",
    version,
    about = "Private repository automation; never publishes crates",
    after_help = "The runner cache lives in target/xtask. All crates remain unpublished."
)]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Execute the accelerated 30-day two-peer storage workload.
    #[cfg(unix)]
    MonthlyWorkload(monthly_workload::Options),
    /// Observe local, captured and follower progress through the native replication CLI.
    #[cfg(unix)]
    ReplicaProgress(replica_progress::Options),
    /// Qualify the actual authenticated snapshot CLIs across two frozen revisions.
    #[cfg(unix)]
    SnapshotCliUpgrade(snapshot_upgrade::Options),
    /// Verify native follower backup, restore and fresh-stream reseeding.
    #[cfg(unix)]
    FollowerBackupRestore(follower_backup::Options),
    /// Exercise a bounded native lifecycle workload on a frozen engine.
    #[cfg(unix)]
    Soak(soak::Options),
    /// Qualify storage or core snapshot compatibility between two archived revisions.
    #[cfg(unix)]
    Compatibility(compatibility::Options),
    /// Compare an archived engine with SQLite using a bounded history workload.
    #[cfg(unix)]
    SqliteBaseline(sqlite_baseline::Options),
    /// Qualify bounded resource and compound workloads on a frozen engine.
    #[cfg(unix)]
    Resource(resource_campaign::Options),
    /// Measure disposable allocation and cache work against a clean reference.
    #[cfg(unix)]
    ResourceProbe(resource_probe::Options),
    /// Exercise near-cap maintenance on a private Docker Linux tmpfs.
    #[cfg(unix)]
    QuotaEnospc(quota_enospc::Options),
    /// Qualify actual EDQUOT and fresh recovery on an owned Linux ext4 mount.
    #[cfg(target_os = "linux")]
    Quota(quota_host::Options),
    #[cfg(target_os = "linux")]
    #[command(hide = true)]
    QuotaGuest(quota_guest::Options),
    /// Compare clean and instrumented serialization on an archived engine.
    #[cfg(unix)]
    SerializationScale(serialization_scale::Options),
    /// Capture samples of the release benchmark and summarize median/range.
    #[cfg(unix)]
    Benchmark(benchmark::Options),
    /// Verify native x64 Windows C/C++ traces against a manifest-pinned DLL.
    #[cfg(windows)]
    WindowsTrace(windows_trace::Options),
    /// Compare delivered Rust/C/Odin consumers with the independent operation model.
    #[cfg(unix)]
    ArtifactTrace(artifact_trace::Options),
    /// Reproduce the macOS C/Rust/Odin packaging experiment with explicit skips.
    #[cfg(unix)]
    Probe(probe::Options),
    /// Detect reviewed safe guard mutants in an isolated source checkout.
    #[cfg(unix)]
    Mutations(mutations::Options),
    /// Produce independent reports for exact parent test executables.
    #[cfg(unix)]
    CoverageExecutables(coverage_executables::Options),
    /// Measure source coverage; reject unavailable metrics or LLVM warnings.
    #[cfg(unix)]
    Coverage(coverage::Options),
    /// Calibrate shared-runtime ASan across C and Rust, then run C consumers.
    #[cfg(unix)]
    CSanitizers(c_sanitizers::Options),
    /// Run ASan or TSan over the workspace with instrumented std.
    #[cfg(unix)]
    Sanitizers(sanitizers::Options),
    /// Run the bounded ownership and budget models and fail-required mutants.
    #[cfg(unix)]
    AtomicModel(atomic_model::Options),
    /// Run discovered fuzz targets with bounded ASan execution and evidence.
    #[cfg(unix)]
    Fuzz(fuzz::Options),
    /// Dynamically load the exact shared library and verify its C ABI.
    AbiSmoke(abi::Options),
    /// Generate raw Odin bindings, or verify them against the C header.
    OdinBindings {
        #[arg(long)]
        check: bool,
    },
    /// Compile C/C++ consumers against an exact Windows DLL/import library.
    WindowsConsumers(toolchain::WindowsConsumers),
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
    #[cfg(target_os = "linux")]
    if let Task::QuotaGuest(options) = cli.task {
        return quota_guest::run(options);
    }
    let root = dunce::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap())?;
    let mut run = Run::new(root.clone());
    match cli.task {
        #[cfg(unix)]
        Task::Resource(options) => resource_campaign::run(&run, options),
        #[cfg(unix)]
        Task::MonthlyWorkload(options) => monthly_workload::run(&run, options),
        #[cfg(unix)]
        Task::ReplicaProgress(options) => replica_progress::run(options),
        #[cfg(unix)]
        Task::SnapshotCliUpgrade(options) => snapshot_upgrade::run(&run, options),
        #[cfg(unix)]
        Task::FollowerBackupRestore(options) => follower_backup::run(&run, options),
        #[cfg(unix)]
        Task::Soak(options) => soak::run(&run, options),
        #[cfg(unix)]
        Task::Compatibility(options) => compatibility::run(&run, options),
        #[cfg(unix)]
        Task::SqliteBaseline(options) => sqlite_baseline::run(&run, options),
        #[cfg(unix)]
        Task::ResourceProbe(options) => resource_probe::run(&run, options),
        #[cfg(unix)]
        Task::QuotaEnospc(options) => quota_enospc::run(&run, options),
        #[cfg(target_os = "linux")]
        Task::Quota(options) => quota_host::run(&run, options),
        #[cfg(target_os = "linux")]
        Task::QuotaGuest(_) => unreachable!(),
        #[cfg(unix)]
        Task::ArtifactTrace(options) => artifact_trace::run(&run, options),
        #[cfg(unix)]
        Task::Benchmark(options) => benchmark::run(&run, options),
        #[cfg(unix)]
        Task::SerializationScale(options) => serialization_scale::run(&run, options),
        #[cfg(unix)]
        Task::Probe(options) => probe::run(&run, options),
        #[cfg(unix)]
        Task::Mutations(options) => mutations::run(&run, options),
        #[cfg(unix)]
        Task::CoverageExecutables(options) => coverage_executables::run(&run, options),
        #[cfg(unix)]
        Task::Coverage(options) => coverage::run(&run, options),
        #[cfg(unix)]
        Task::CSanitizers(options) => c_sanitizers::run(&run, options),
        #[cfg(unix)]
        Task::Sanitizers(options) => sanitizers::run(&run, options),
        #[cfg(unix)]
        Task::AtomicModel(options) => atomic_model::run(&run, options),
        #[cfg(unix)]
        Task::Fuzz(options) => fuzz::run(&run, options),
        Task::AbiSmoke(options) => abi::run(options),
        Task::OdinBindings { check } => odin::run(&root, check),
        Task::WindowsConsumers(options) => toolchain::windows_consumers(&mut run, options),
        #[cfg(windows)]
        Task::WindowsTrace(options) => windows_trace::run(&mut run, options),
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
    #[cfg(unix)]
    #[test]
    fn native_fault_cli_preserves_source_output_and_help() {
        let cli = Cli::try_parse_from([
            "xtask", "faults", "native", "--source", "HEAD", "--output", "new",
        ])
        .unwrap();
        let Task::Faults {
            task: faults::Task::Native(_),
        } = cli.task
        else {
            panic!("native task")
        };
        use clap::error::ErrorKind;
        assert_eq!(
            Cli::try_parse_from(["xtask", "faults", "native", "--help"])
                .err()
                .unwrap()
                .kind(),
            ErrorKind::DisplayHelp
        );
    }
}
