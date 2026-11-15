use crate::{
    assurance::{self, Campaign},
    instrumentation::environment,
    support::{Run, cmd},
};
use anyhow::Result;
use clap::{Args, ValueEnum};
use std::path::PathBuf;
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Kind {
    Address,
    Thread,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Address => "address",
            Self::Thread => "thread",
        }
    }
}
#[derive(Debug, Args)]
pub struct Options {
    #[arg(value_enum)]
    pub kind: Kind,
    #[arg(long, default_value = "nightly")]
    pub toolchain: String,
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long)]
    pub output: Option<PathBuf>,
}
pub fn run(run: &Run, options: Options) -> Result<()> {
    let output = options.output.unwrap_or_else(|| {
        PathBuf::from(format!("target/sanitizer-{}-evidence", options.kind.name()))
    });
    let mut c = Campaign::new(
        run,
        &output,
        "Workspace sanitizer with instrumented std, including xtask; diagnostic artifact only",
    )?;
    let result = (|| {
        let toolchain = format!("+{}", options.toolchain);
        let version = c.command(
            cmd(["rustc", &toolchain, "-vV"]),
            "rustc-version",
            None,
            30.,
        )?;
        let target = options
            .target
            .unwrap_or(assurance::host(&version)?.to_owned());
        c.evidence["rustc"] = serde_json::json!(version);
        let flags = vec![format!("-Zsanitizer={}", options.kind.name())];
        c.env = environment(
            &run.env,
            &run.root
                .join(format!("target/sanitizer-{}", options.kind.name())),
            &flags,
        );
        c.command(
            cmd([
                "cargo",
                &toolchain,
                "test",
                "--locked",
                "-Zbuild-std",
                "--workspace",
                "--all-features",
                "--target",
                &target,
            ]),
            "workspace",
            None,
            match options.kind {
                Kind::Thread => 2400.,
                Kind::Address => 1200.,
            },
        )?;
        Ok(())
    })();
    c.finish(result)
}
