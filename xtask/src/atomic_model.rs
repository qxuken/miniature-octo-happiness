use crate::{
    assurance::Campaign,
    support::{Run, cmd, path},
};
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
#[derive(Debug, Args)]
pub struct Options {
    #[arg(long, default_value = "target/atomic-model-evidence")]
    pub output: PathBuf,
    #[arg(long)]
    pub toolchain: Option<String>,
}
pub fn run(run: &Run, mut options: Options) -> Result<()> {
    if options.toolchain.is_none() {
        options.toolchain = run
            .env
            .get(&path("MODEL_TOOLCHAIN"))
            .map(|v| v.to_string_lossy().into_owned());
    }
    let mut c = Campaign::new(
        run,
        &options.output,
        "Bounded ownership, budget and FFI models with fail-required controls",
    )?;
    c.env.insert(path("CARGO_INCREMENTAL"), path("0"));
    c.env
        .insert(path("CARGO_TARGET_DIR"), path(c.output.join("build")));
    let result = (|| {
        let mut rustc = cmd(["rustc"]);
        if let Some(t) = &options.toolchain {
            rustc.push(path(format!("+{t}")));
        }
        rustc.push(path("-vV"));
        let version = c.command(rustc, "rustc-version", None, 30.)?;
        c.evidence["rustc"] = serde_json::json!(version);
        for (binary, mode, expected) in [
            ("weekend-atomic-model", "baseline", None),
            ("weekend-atomic-model", "ffi-handles", None),
            (
                "weekend-atomic-model",
                "broken-ffi-reservation",
                Some("only successful handle reservations may survive quiescence"),
            ),
            (
                "weekend-atomic-model",
                "broken-ffi-rollback",
                Some("only successful handle reservations may survive quiescence"),
            ),
            ("budget", "baseline", None),
            (
                "budget",
                "broken-admission",
                Some("no released reservation may be forgotten"),
            ),
            (
                "budget",
                "broken-sampling",
                Some("observed peak must not decrease"),
            ),
        ] {
            let mut command = cmd(["cargo"]);
            if let Some(t) = &options.toolchain {
                command.push(path(format!("+{t}")));
            }
            command.extend(cmd([
                "run",
                "--locked",
                "--manifest-path",
                "tools/atomic-model/Cargo.toml",
                "--bin",
                binary,
                "--",
            ]));
            if mode != "baseline" {
                command.push(path(format!("--{mode}")));
            }
            c.command(command, &format!("{binary}-{mode}"), expected, 600.)?;
        }
        Ok(())
    })();
    c.finish(result)
}
