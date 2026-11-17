//! Calibrate both sides of a shared-runtime ASan ABI before trusting consumers.
use crate::{
    assurance::{self, Campaign},
    instrumentation,
    support::{Run, cmd, path, sha},
};
use anyhow::{Result, ensure};
use clap::Args;
use serde_json::json;
use std::{fs, path::PathBuf};
#[derive(Debug, Args)]
pub struct Options {
    #[arg(long, default_value = "target/c-sanitizer-evidence")]
    pub output: PathBuf,
    #[arg(long, default_value = "clang")]
    pub cc: String,
    #[arg(long, default_value = "nightly")]
    pub toolchain: String,
}
const CALIBRATION_RUST: &str = r#"#[unsafe(no_mangle)]
#[inline(never)]
pub unsafe extern "C" fn calibrated_read(p: *const u8, index: usize) -> u8 {
    unsafe { p.add(index).read_volatile() }
}
"#;
const CALIBRATION_C: &str = r#"#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
extern uint8_t calibrated_read(const uint8_t *, size_t);
int main(int argc, char **argv) {
    assert(argc == 2);
    uint8_t *p = malloc(8);
    assert(p != NULL);
    for (size_t i = 0; i < 8; ++i) p[i] = 0;
    volatile size_t index = strstr(argv[1], "invalid") == NULL ? 7 : 8;
    volatile uint8_t result = argv[1][0] == 'c' ? p[index] : calibrated_read(p, index);
    free(p);
    return result;
}
"#;
pub fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        cfg!(any(target_os = "macos", target_os = "linux")),
        "C ASan supports native macOS/Linux only"
    );
    let mut c = Campaign::new(
        run,
        &options.output,
        "Shared-runtime C/Rust ASan calibration and memory/thread/file consumers; diagnostic build, not release artifact",
    )?;
    let result = (|| {
        let clang = c.command(cmd([&options.cc, "--version"]), "clang-version", None, 30.)?;
        ensure!(
            clang.to_lowercase().contains("clang"),
            "Clang shared ASan runtime required"
        );
        c.evidence["clang"] = json!(clang);
        let toolchain = format!("+{}", options.toolchain);
        let version = c.command(
            cmd(["rustc", &toolchain, "-vV"]),
            "rustc-version",
            None,
            30.,
        )?;
        let target = assurance::host(&version)?.to_owned();
        c.evidence["rustc"] = json!(version);
        let arch = target.split('-').next().unwrap();
        let runtime_name = if cfg!(target_os = "macos") {
            "libclang_rt.asan_osx_dynamic.dylib".into()
        } else {
            format!("libclang_rt.asan-{arch}.so")
        };
        let runtime = c.command(
            cmd([&options.cc, &format!("-print-file-name={runtime_name}")]),
            "asan-runtime",
            None,
            30.,
        )?;
        let runtime = dunce::canonicalize(PathBuf::from(runtime))?;
        ensure!(runtime.is_file(), "shared ASan runtime must be a file");
        c.evidence["runtime"] = json!({"path":runtime,"sha256":sha(&runtime)?});
        let flags = vec![
            "-Zsanitizer=address".into(),
            "-Zexternal-clangrt".into(),
            "-C".into(),
            format!("link-arg={}", runtime.display()),
        ];
        c.env = instrumentation::address_environment(&run.env, &c.output.join("build"), &flags);
        c.evidence["rust_flags"] = json!(flags);
        c.evidence["asan_options"] = json!(c.env[&path("ASAN_OPTIONS")].to_str());
        let mut cflags = cmd([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-g",
            "-O1",
            "-fsanitize=address",
            "-fno-omit-frame-pointer",
        ]);
        if cfg!(target_os = "linux") {
            cflags.push(path("-shared-libasan"));
        }
        cflags.push(path(format!(
            "-Wl,-rpath,{}",
            runtime.parent().unwrap().display()
        )));
        let ext = if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };
        let rust = c.output.join("calibration.rs");
        fs::write(&rust, CALIBRATION_RUST)?;
        let library = c.output.join(format!("libcalibration.{ext}"));
        let mut command = cmd(["rustc", &toolchain, "--edition=2024", "--crate-type=cdylib"]);
        command.extend(flags.iter().map(path));
        command.extend([path(&rust), path("-o"), path(&library)]);
        c.command(command, "calibration-rust-build", None, 600.)?;
        let source = c.output.join("calibration.c");
        fs::write(&source, CALIBRATION_C)?;
        let calibration = c.output.join("calibration");
        let mut command = cmd([&options.cc]);
        command.extend(cflags.clone());
        command.extend([
            path(source),
            path(&library),
            path(format!("-Wl,-rpath,{}", c.output.display())),
            path("-o"),
            path(&calibration),
        ]);
        c.command(command, "calibration-c-build", None, 600.)?;
        c.evidence["calibration"] = json!([]);
        for mode in ["c-valid", "rust-valid", "c-invalid", "rust-invalid"] {
            let expected = mode
                .ends_with("invalid")
                .then_some("AddressSanitizer: heap-buffer-overflow");
            c.command(
                vec![path(&calibration), path(mode)],
                &format!("calibration-{mode}"),
                expected,
                600.,
            )?;
            c.evidence["calibration"]
                .as_array_mut()
                .unwrap()
                .push(json!({"mode":mode,"passed":true}));
            c.save()?;
        }
        c.command(
            cmd([
                "cargo",
                &toolchain,
                "build",
                "--locked",
                "-Zbuild-std",
                "--lib",
                "--features",
                "crdt,replication",
                "--target",
                &target,
            ]),
            "rust-library-build",
            None,
            600.,
        )?;
        let library = c
            .output
            .join("build")
            .join(&target)
            .join("debug")
            .join(format!("libweekend_db.{ext}"));
        c.evidence["library"] = json!({"path":library,"sha256":sha(&library)?});
        c.evidence["consumers"] = json!([]);
        for name in ["memory", "threads", "file"] {
            let binary = c.output.join(format!("c-{name}"));
            let mut command = cmd([&options.cc]);
            command.extend(cflags.clone());
            command.extend([
                path("-pthread"),
                path("-I"),
                path(run.root.join("include")),
                path(run.root.join(format!("examples/{name}.c"))),
                path(&library),
                path(format!(
                    "-Wl,-rpath,{}",
                    library.parent().unwrap().display()
                )),
                path("-o"),
                path(&binary),
            ]);
            c.command(command, &format!("{name}-build"), None, 600.)?;
            let mut command = vec![path(&binary)];
            if name == "file" {
                command.push(path(c.output.join("file.wdb")));
            }
            c.command(command, &format!("{name}-run"), None, 600.)?;
            c.evidence["consumers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name":name,"sha256":sha(&binary)?,"passed":true}));
            c.save()?;
        }
        Ok(())
    })();
    c.finish(result)
}
