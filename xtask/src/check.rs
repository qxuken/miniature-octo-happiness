use crate::support::{Platform, Run, Scratch, cmd, path};
use anyhow::Result;
use std::{
    ffi::{OsStr, OsString},
    path::{Component, Path},
};

fn relative(from: &Path, to: &Path) -> OsString {
    let a: Vec<Component<'_>> = from.components().collect();
    let b: Vec<Component<'_>> = to.components().collect();
    let common = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let mut result = std::path::PathBuf::new();
    for _ in common..a.len() {
        result.push("..");
    }
    for part in &b[common..] {
        result.push(part.as_os_str());
    }
    path(result)
}

pub fn prelude() -> Vec<Vec<OsString>> {
    vec![
        cmd(["cargo", "fmt", "--all", "--check"]),
        cmd([
            "cargo",
            "clippy",
            "--offline",
            "--workspace",
            "--all-features",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]),
    ]
}

#[allow(clippy::too_many_arguments)]
pub fn profile(
    root: &Path,
    target: &Path,
    platform: Platform,
    arch: &str,
    skip: bool,
    xtask: &OsStr,
    scratch: &Path,
    release: bool,
) -> Vec<Vec<OsString>> {
    let mode = if release { "release" } else { "debug" };
    let dest = target.join(mode);
    let library = dest.join(platform.library());
    let mut rows = vec![];
    let cargo = |base: &[&str]| {
        let mut v = cmd(["cargo"]);
        v.extend(cmd(base));
        if release {
            v.push(path("--release"));
        }
        v
    };
    rows.push(cargo(&[
        "test",
        "--offline",
        "--workspace",
        "--all-features",
    ]));
    let mut demo = cargo(&["run", "--offline", "-p", "weekend-tasks"]);
    demo.extend(cmd(["--", "demo"]));
    rows.push(demo);
    let mut renamed = cmd(["cargo", "run", "--offline", "--manifest-path"]);
    renamed.push(path(
        root.join("crates/weekend-db-typed/tests/renamed/Cargo.toml"),
    ));
    renamed.extend([path("--target-dir"), path(target.join("renamed-consumer"))]);
    if release {
        renamed.push(path("--release"));
    }
    rows.push(renamed);
    for example in ["memory", "raw_tasks", "simulated", "typed"] {
        rows.push(cargo(&["run", "--offline", "--example", example]));
    }
    rows.push(cargo(&["build", "--offline", "--workspace"]));
    rows.push(vec![path(xtask), path("abi-smoke"), path(&library)]);
    let mut rust_file = |example: &str, file: &str| {
        let mut v = cargo(&["run", "--offline", "--example", example]);
        v.extend([path("--"), path(scratch.join(file))]);
        rows.push(v);
    };
    if platform == Platform::Windows {
        rust_file("file", "rust.db");
        rust_file("typed", "typed.db");
        return rows;
    }
    if !matches!(platform, Platform::Mac | Platform::Linux) {
        return rows;
    }
    for (name, example, threaded) in [
        ("c-memory", "memory.c", false),
        ("c-threads", "threads.c", true),
        ("c-file", "file.c", false),
    ] {
        if name == "c-file" && platform == Platform::Linux && !matches!(arch, "x86_64" | "aarch64")
        {
            return rows;
        }
        let mut cc = cmd(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror"]);
        if threaded {
            cc.push(path("-pthread"));
        }
        cc.extend([
            path("-I"),
            path(root.join("include")),
            path(root.join("examples").join(example)),
            path(&library),
            path(format!("-Wl,-rpath,{}", dest.display())),
            path("-o"),
            path(dest.join(name)),
        ]);
        rows.push(cc);
        if name == "c-file" {
            for (example, file) in [("file", "rust.db"), ("typed", "typed.db")] {
                let mut v = cargo(&["run", "--offline", "--example", example]);
                v.extend([path("--"), path(scratch.join(file))]);
                rows.push(v);
            }
            rows.push(vec![path(dest.join(name)), path(scratch.join("c.db"))]);
        } else {
            rows.push(vec![path(dest.join(name))]);
        }
    }
    if platform == Platform::Mac && !skip {
        for (folder, name) in [("odin", "odin-memory"), ("odin_file", "odin-file")] {
            rows.push(vec![
                path("odin"),
                path("build"),
                path(root.join("examples").join(folder)),
                path(format!("-out:{}", dest.join(name).display())),
                path(format!(
                    "-define:ENGINE_LIB={}",
                    relative(&root.join("examples").join(folder), &library).to_string_lossy()
                )),
                path(format!("-extra-linker-flags:-Wl,-rpath,{}", dest.display())),
            ]);
            let mut command = vec![path(dest.join(name))];
            if name == "odin-file" {
                command.push(path(scratch.join("odin.db")));
            }
            rows.push(command);
        }
    }
    rows
}

pub fn run(run: &mut Run, platform: Platform, skip: bool, dry: bool) -> Result<()> {
    let target = run.target();
    let executable = std::env::current_exe()?;
    if dry {
        let mut commands = prelude();
        for release in [false, true] {
            commands.extend(profile(
                &run.root,
                &target,
                platform,
                std::env::consts::ARCH,
                skip,
                executable.as_os_str(),
                Path::new("<temporary>"),
                release,
            ));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(
                &commands
                    .iter()
                    .map(|c| c.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            )?
        );
        return Ok(());
    }
    for command in prelude() {
        run.status(&command)?;
    }
    for release in [false, true] {
        let mut scratch = Scratch::new(&std::env::temp_dir(), "checks")?;
        for command in profile(
            &run.root,
            &target,
            platform,
            std::env::consts::ARCH,
            skip,
            executable.as_os_str(),
            scratch.path(),
            release,
        ) {
            run.status(&command)?;
        }
        scratch.close()?;
    }
    println!(
        "Workspace debug/release, CLI, renamed consumer and native ABI checks passed{}.",
        if skip {
            "; Odin explicitly skipped"
        } else {
            ""
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baseline_python_command_plans_are_preserved() {
        let cases = [
            (Platform::Mac, "aarch64", false, "mac"),
            (Platform::Mac, "aarch64", true, "mac-skip"),
            (Platform::Linux, "aarch64", false, "linux"),
            (Platform::Linux, "riscv64", false, "linux-other"),
            (Platform::Windows, "x86_64", false, "windows"),
            (Platform::Other, "x86_64", false, "other"),
        ];
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../tests/check-plans.json")).unwrap();
        for (platform, arch, skip, name) in cases {
            let mut commands = prelude();
            for release in [false, true] {
                commands.extend(profile(
                    Path::new("/repo"),
                    Path::new("/repo/target"),
                    platform,
                    arch,
                    skip,
                    OsStr::new("<xtask>"),
                    Path::new("/scratch"),
                    release,
                ));
            }
            // Preserve the historical command graph, with exactly the ABI
            // Python consumer replaced by its independent Rust dynamic consumer.
            let normalized: Vec<Vec<String>> = commands
                .iter()
                .map(|row| {
                    if row[0] == OsStr::new("<xtask>") {
                        assert_eq!(row[1], OsStr::new("abi-smoke"));
                        let mut old =
                            vec!["python".to_owned(), "/repo/tools/ffi_smoke.py".to_owned()];
                        old.extend(
                            row[2..]
                                .iter()
                                .map(|s| s.to_string_lossy().replace('\\', "/")),
                        );
                        old
                    } else {
                        row.iter()
                            .map(|s| s.to_string_lossy().replace('\\', "/"))
                            .collect()
                    }
                })
                .collect();
            assert_eq!(
                serde_json::to_value(normalized).unwrap(),
                fixtures[name],
                "{name}"
            );
        }
    }
}
