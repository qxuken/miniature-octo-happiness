//! Preserve the milestone-one C/Rust/Odin packaging experiment, with bounded
//! processes and exact diagnostics for its deliberate failure controls.
use crate::{
    assurance::{self, Campaign},
    instrumentation,
    support::{Run, cmd, path, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::json;
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};
#[derive(Debug, Args)]
pub struct Options {
    #[arg(long)]
    pub skip_odin: bool,
    #[arg(long)]
    pub skip_wgpu: bool,
    #[arg(long, default_value = "target/packaging-probe")]
    pub output: PathBuf,
}
fn amalgam(header: &str, body: &str) -> String {
    format!(
        "{header}\n#if defined(WEEKEND_PROBE_IMPLEMENTATION) && !defined(WEEKEND_PROBE_IMPLEMENTED)\n#define WEEKEND_PROBE_IMPLEMENTED\n{}\n#endif\n",
        body.replacen("#include \"weekend_probe.h\"\n", "", 1)
    )
}
fn labels(skip_odin: bool, skip_wgpu: bool) -> Vec<String> {
    let mut out = vec![];
    let mut libraries = vec![];
    for mode in ["debug", "release"] {
        for kind in ["c-static", "c-shared", "rust-static", "rust-shared"] {
            let label = format!("{kind}-{mode}");
            out.push(label.clone());
            libraries.push(label);
        }
    }
    out.extend(
        [
            "c-amalgamated-two-consumers",
            "c-sanitize-contract",
            "c-sanitize-allocation",
            "c-lifetime-mistake",
            "rust-lifetime-mistake",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    if !skip_odin {
        for library in libraries {
            out.push(format!("{library}-plain"));
            if !skip_wgpu {
                out.push(format!("{library}-wgpu"));
            }
        }
    }
    out
}
fn duplicate_result(
    label: &str,
    with_wgpu: bool,
    report: &crate::supervise::Report,
    log: &str,
) -> Result<&'static str> {
    if report.success() {
        return Ok("pass");
    }
    ensure!(
        label.starts_with("rust-static")
            && with_wgpu
            && assurance::qualifies(report, log, Some("duplicate symbol")),
        "unexpected Odin failure for {label}"
    );
    Ok("duplicate-symbol failure")
}
fn expected(c: &mut Campaign<'_>, args: Vec<OsString>, name: &str, marker: &str) -> Result<()> {
    let _ = c.command(args, name, None, 120.);
    let report: crate::supervise::Report = serde_json::from_value(
        c.evidence["commands"].as_array().unwrap().last().unwrap()["supervision"].clone(),
    )
    .context("expected-failure process report")?;
    let mut log = fs::read(c.output.join(format!("{name}.stdout.log")))?;
    log.extend(fs::read(c.output.join(format!("{name}.stderr.log")))?);
    let passed = assurance::qualifies(&report, &String::from_utf8_lossy(&log), Some(marker));
    let row = c.evidence["commands"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    row["passed"] = json!(passed);
    row["expected_assertion"] = json!(marker);
    row["expected_assertion_stream"] = json!("combined");
    c.save()?;
    ensure!(passed, "expected failure not diagnosed for {name}");
    c.evidence["results"]
        .as_array_mut()
        .unwrap()
        .push(json!({"case":name,"diagnostic":"pass"}));
    Ok(())
}
fn consumers(
    c: &mut Campaign<'_>,
    root: &Path,
    label: &str,
    library: &Path,
    rust_static: bool,
) -> Result<()> {
    let dest = c.output.join(label);
    fs::create_dir(&dest)?;
    let native = if rust_static {
        cmd([
            "-framework",
            "Security",
            "-framework",
            "CoreFoundation",
            "-liconv",
        ])
    } else {
        vec![]
    };
    let mut args = cmd(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I"]);
    args.push(path(root.join("include")));
    args.extend([
        path(root.join("consumers/contract.c")),
        path(root.join("consumers/main.c")),
        path(library),
    ]);
    args.extend(native.clone());
    args.extend([
        path(format!(
            "-Wl,-rpath,{}",
            library.parent().unwrap().display()
        )),
        path("-o"),
        path(dest.join("c-consumer")),
    ]);
    c.command(args, &format!("{label}-c-build"), None, 120.)?;
    c.command(
        vec![path(dest.join("c-consumer"))],
        &format!("{label}-c-run"),
        None,
        120.,
    )?;
    let mut args = cmd(["rustc", "--edition=2024"]);
    args.extend([
        path(root.join("consumers/rust.rs")),
        path("-o"),
        path(dest.join("rust-consumer")),
        path("-C"),
        path(format!("link-arg={}", library.display())),
        path("-C"),
        path(format!(
            "link-arg=-Wl,-rpath,{}",
            library.parent().unwrap().display()
        )),
    ]);
    for arg in native {
        args.extend([
            path("-C"),
            path(format!(
                "link-arg={}",
                arg.to_str().context("native link flag")?
            )),
        ]);
    }
    c.command(args, &format!("{label}-rust-build"), None, 120.)?;
    c.command(
        vec![path(dest.join("rust-consumer"))],
        &format!("{label}-rust-run"),
        None,
        120.,
    )?;
    c.evidence["results"]
        .as_array_mut()
        .unwrap()
        .push(json!({"case":label,"c":"pass","rust":"pass","library_sha256":sha(library)?}));
    c.save()?;
    Ok(())
}
fn odin(
    c: &mut Campaign<'_>,
    root: &Path,
    label: &str,
    library: &Path,
    with_wgpu: bool,
) -> Result<()> {
    let name = format!("{label}-{}", if with_wgpu { "wgpu" } else { "plain" });
    let source = c.output.join(format!("{name}-source"));
    fs::create_dir(&source)?;
    fs::copy(
        root.join("consumers/odin/main.odin"),
        source.join("main.odin"),
    )?;
    fs::copy(
        root.join(if with_wgpu {
            "consumers/integration/wgpu.odin"
        } else {
            "consumers/integration/plain.odin"
        }),
        source.join("integration.odin"),
    )?;
    let binary = c.output.join(format!("{name}-odin"));
    let relative = PathBuf::from("..").join(library.strip_prefix(&c.output)?);
    let stage = format!("{name}-build");
    let _ = c.command(
        vec![
            path("odin"),
            path("build"),
            path(&source),
            path(format!("-out:{}", binary.display())),
            path(format!("-define:PROBE_LIB={}", relative.display())),
            path(format!(
                "-extra-linker-flags:-Wl,-rpath,{}",
                library.parent().unwrap().display()
            )),
        ],
        &stage,
        None,
        120.,
    );
    let report: crate::supervise::Report = serde_json::from_value(
        c.evidence["commands"].as_array().unwrap().last().unwrap()["supervision"].clone(),
    )?;
    let mut log = fs::read(c.output.join(format!("{stage}.stdout.log")))?;
    log.extend(fs::read(c.output.join(format!("{stage}.stderr.log")))?);
    let outcome = duplicate_result(label, with_wgpu, &report, &String::from_utf8_lossy(&log))?;
    if outcome == "pass" {
        c.command(vec![path(binary)], &format!("{name}-run"), None, 120.)?;
    } else {
        let row = c.evidence["commands"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap();
        row["passed"] = json!(true);
        row["expected_assertion"] = json!("duplicate symbol");
        row["expected_assertion_stream"] = json!("combined");
    }
    c.evidence["results"]
        .as_array_mut()
        .unwrap()
        .push(json!({"case":name,"odin":outcome}));
    c.save()?;
    Ok(())
}
pub fn run(run: &Run, options: Options) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "the original packaging experiment is macOS-only"
    );
    for tool in ["cc", "ar", "rustc", "cargo"]
        .into_iter()
        .chain((!options.skip_odin).then_some("odin"))
    {
        ensure!(
            toolchain::which(OsStr::new(tool), &run.env).is_some(),
            "missing tool {tool}; select only applicable explicit skips"
        );
    }
    let root = run.root.join("probe");
    let mut c = Campaign::new(
        run,
        &options.output,
        "Milestone-one C/Rust packaging experiment; local build paths, not engine or release-artifact qualification",
    )?;
    c.env = instrumentation::environment(&run.env, &c.output.join("rust-target"), &[]);
    c.env.insert(
        path("ASAN_OPTIONS"),
        path("halt_on_error=1:abort_on_error=0:exitcode=86:detect_leaks=0"),
    );
    c.env.insert(
        path("UBSAN_OPTIONS"),
        path("halt_on_error=1:print_stacktrace=1"),
    );
    c.evidence["results"] = json!([]);
    let result = (|| {
        let mut environment = json!({"machine":std::env::consts::ARCH,"skipped":[]});
        for (name, command) in [
            ("platform", cmd(["uname", "-a"])),
            ("rustc", cmd(["rustc", "-vV"])),
            ("cargo", cmd(["cargo", "--version"])),
            ("cc", cmd(["cc", "--version"])),
        ] {
            environment[name] = json!(c.command(command, &format!("version-{name}"), None, 30.)?);
        }
        if options.skip_odin {
            environment["skipped"]
                .as_array_mut()
                .unwrap()
                .push(json!("odin"));
        } else {
            environment["odin"] =
                json!(c.command(cmd(["odin", "version"]), "version-odin", None, 30.)?);
        }
        if options.skip_wgpu {
            environment["skipped"]
                .as_array_mut()
                .unwrap()
                .push(json!("wgpu"));
        }
        let archive = if !options.skip_odin && !options.skip_wgpu {
            let home = c.command(cmd(["odin", "root"]), "odin-root", None, 30.)?;
            let arch = if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else {
                "x86_64"
            };
            let archive = PathBuf::from(home).join(format!(
                "vendor/wgpu/lib/wgpu-macos-{arch}-release/lib/libwgpu_native.a"
            ));
            let hash = sha(&archive)?;
            environment["wgpu_archive"] = json!(archive);
            environment["wgpu_sha256"] = json!(hash);
            Some((archive, hash))
        } else {
            None
        };
        c.evidence["environment"] = environment;
        c.save()?;
        let amalgamated = c.output.join("amalgamated");
        fs::create_dir(&amalgamated)?;
        fs::write(
            amalgamated.join("weekend_probe.h"),
            amalgam(
                &fs::read_to_string(root.join("include/weekend_probe.h"))?,
                &fs::read_to_string(root.join("c/weekend_probe.c"))?,
            ),
        )?;
        fs::write(
            amalgamated.join("implementation.c"),
            "#define WEEKEND_PROBE_IMPLEMENTATION\n#include \"weekend_probe.h\"\n",
        )?;
        let mut libraries = vec![];
        for (mode, flags) in [("debug", cmd(["-O0", "-g"])), ("release", cmd(["-O2"]))] {
            let dest = c.output.join(format!("c-{mode}"));
            fs::create_dir(&dest)?;
            let mut common = cmd(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror"]);
            common.extend(flags);
            common.extend([path("-I"), path(root.join("include"))]);
            let mut compile = common.clone();
            compile.extend([
                path("-c"),
                path(root.join("c/weekend_probe.c")),
                path("-o"),
                path(dest.join("probe.o")),
            ]);
            c.command(compile, &format!("c-{mode}-object"), None, 120.)?;
            c.command(
                vec![
                    path("ar"),
                    path("rcs"),
                    path(dest.join("libweekend_probe.a")),
                    path(dest.join("probe.o")),
                ],
                &format!("c-{mode}-archive"),
                None,
                120.,
            )?;
            common.extend([
                path("-dynamiclib"),
                path(root.join("c/weekend_probe.c")),
                path("-o"),
                path(dest.join("libweekend_probe.dylib")),
            ]);
            c.command(common, &format!("c-{mode}-shared"), None, 120.)?;
            for (kind, name) in [
                ("static", "libweekend_probe.a"),
                ("shared", "libweekend_probe.dylib"),
            ] {
                let label = format!("c-{kind}-{mode}");
                let library = dest.join(name);
                consumers(&mut c, &root, &label, &library, false)?;
                libraries.push((label, library));
            }
            let mut build = cmd(["cargo", "build", "--offline", "--locked", "--manifest-path"]);
            build.push(path(root.join("rust/Cargo.toml")));
            if mode == "release" {
                build.push(path("--release"));
            }
            c.command(build, &format!("rust-{mode}-library-build"), None, 120.)?;
            for (kind, name) in [
                ("static", "libweekend_probe.a"),
                ("shared", "libweekend_probe.dylib"),
            ] {
                let label = format!("rust-{kind}-{mode}");
                let library = c.output.join("rust-target").join(mode).join(name);
                consumers(&mut c, &root, &label, &library, kind == "static")?;
                libraries.push((label, library));
            }
        }
        let amalgam_binary = amalgamated.join("consumer");
        c.command(
            vec![
                path("cc"),
                path("-std=c11"),
                path("-Wall"),
                path("-Wextra"),
                path("-Werror"),
                path("-I"),
                path(&amalgamated),
                path(amalgamated.join("implementation.c")),
                path(root.join("consumers/contract.c")),
                path(root.join("consumers/main.c")),
                path("-o"),
                path(&amalgam_binary),
            ],
            "amalgamated-build",
            None,
            120.,
        )?;
        c.command(vec![path(amalgam_binary)], "amalgamated-run", None, 120.)?;
        c.evidence["results"]
            .as_array_mut()
            .unwrap()
            .push(json!({"case":"c-amalgamated-two-consumers","c":"pass"}));
        for (name, sources) in [
            (
                "contract",
                vec![
                    root.join("c/weekend_probe.c"),
                    root.join("consumers/contract.c"),
                    root.join("consumers/main.c"),
                ],
            ),
            ("allocation", vec![root.join("consumers/allocation.c")]),
        ] {
            let binary = c.output.join(format!("sanitize-{name}"));
            let mut args = cmd([
                "cc",
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-g",
                "-fsanitize=address,undefined",
                "-fno-omit-frame-pointer",
                "-I",
            ]);
            args.push(path(root.join("include")));
            args.extend(sources.iter().map(path));
            args.extend([path("-o"), path(&binary)]);
            c.command(args, &format!("sanitize-{name}-build"), None, 120.)?;
            c.command(
                vec![path(binary)],
                &format!("sanitize-{name}-run"),
                None,
                120.,
            )?;
            c.evidence["results"]
                .as_array_mut()
                .unwrap()
                .push(json!({"case":format!("c-sanitize-{name}"),"c":"pass"}));
        }
        let binary = c.output.join("c-lifetime-mistake");
        c.command(
            vec![
                path("cc"),
                path("-std=c11"),
                path("-g"),
                path("-fsanitize=address"),
                path("-I"),
                path(root.join("include")),
                path(root.join("c/weekend_probe.c")),
                path(root.join("consumers/lifetime.c")),
                path("-o"),
                path(&binary),
            ],
            "c-lifetime-build",
            None,
            120.,
        )?;
        expected(
            &mut c,
            vec![path(binary)],
            "c-lifetime-mistake",
            "heap-use-after-free",
        )?;
        let bad = c.output.join("rust-lifetime-mistake");
        expected(
            &mut c,
            vec![
                path("rustc"),
                path("--edition=2024"),
                path("--cfg"),
                path("lifetime_mistake"),
                path(root.join("consumers/rust.rs")),
                path("-o"),
                path(bad),
            ],
            "rust-lifetime-mistake",
            "does not live long enough",
        )?;
        if !options.skip_odin {
            for (label, library) in libraries {
                odin(&mut c, &root, &label, &library, false)?;
                if !options.skip_wgpu {
                    odin(&mut c, &root, &label, &library, true)?;
                }
            }
        }
        if let Some((file, hash)) = archive {
            ensure!(sha(&file)? == hash, "wgpu archive changed");
        }
        let actual = c.evidence["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                r["case"]
                    .as_str()
                    .context("probe case name")
                    .map(str::to_owned)
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            actual == labels(options.skip_odin, options.skip_wgpu),
            "probe case inventory changed"
        );
        fs::write(
            c.output.join("results.json"),
            serde_json::to_vec_pretty(
                &json!({"environment":c.evidence["environment"],"results":c.evidence["results"]}),
            )?,
        )?;
        Ok(())
    })();
    c.finish(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_inventory_preserves_29_cases_and_explicit_skips() {
        assert_eq!(labels(false, false).len(), 29);
        assert_eq!(labels(false, true).len(), 21);
        assert_eq!(labels(true, false).len(), 13);
        let full = labels(false, false);
        assert_eq!(
            full.iter().collect::<std::collections::BTreeSet<_>>().len(),
            29
        );
        assert!(full.contains(&"rust-static-release-wgpu".into()));
    }
    #[test]
    fn duplicate_symbols_are_only_expected_for_rust_static_with_wgpu() {
        let mut r = crate::supervise::Report {
            returncode: Some(1),
            cleanup_verified: true,
            failure: Some("child exit Some(1)".into()),
            ..Default::default()
        };
        assert_eq!(
            duplicate_result("rust-static-debug", true, &r, "duplicate symbol").unwrap(),
            "duplicate-symbol failure"
        );
        for (label, wgpu, log) in [
            ("rust-shared-debug", true, "duplicate symbol"),
            ("rust-static-debug", false, "duplicate symbol"),
            ("rust-static-debug", true, "undefined symbol"),
        ] {
            assert!(duplicate_result(label, wgpu, &r, log).is_err());
        }
        r.timed_out = true;
        assert!(duplicate_result("rust-static-debug", true, &r, "duplicate symbol").is_err());
        r.timed_out = false;
        r.cleanup_verified = false;
        assert!(duplicate_result("rust-static-debug", true, &r, "duplicate symbol").is_err());
        r = crate::supervise::Report {
            returncode: Some(0),
            cleanup_verified: true,
            ..Default::default()
        };
        assert_eq!(
            duplicate_result("rust-shared-debug", true, &r, "").unwrap(),
            "pass"
        );
    }
}
