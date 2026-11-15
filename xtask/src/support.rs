use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn path(value: impl AsRef<OsStr>) -> OsString {
    value.as_ref().to_owned()
}
pub fn cmd<I, S>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter().map(path).collect()
}
pub fn sha(file: &Path) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(fs::read(file).with_context(|| format!("read {}", file.display()))?)
    ))
}
pub fn text(value: &Value) -> Result<&str> {
    value.as_str().context("expected JSON string")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Linux,
    Windows,
    Other,
}
impl Platform {
    pub fn host() -> Result<Self> {
        Ok(if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Other
        })
    }
    pub fn library(self) -> &'static str {
        match self {
            Self::Mac => "libweekend_db.dylib",
            Self::Linux => "libweekend_db.so",
            Self::Windows => "weekend_db.dll",
            Self::Other => "libweekend_db.so",
        }
    }
}

pub struct Run {
    pub root: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub commands: Vec<Value>,
    pub log_dir: Option<PathBuf>,
    #[cfg(windows)]
    pub private_deadline: Option<std::time::Instant>,
}
impl Run {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            env: env::vars_os().collect(),
            commands: vec![],
            log_dir: None,
            #[cfg(windows)]
            private_deadline: None,
        }
    }
    pub fn target(&self) -> PathBuf {
        let p = self
            .env
            .get(OsStr::new("CARGO_TARGET_DIR"))
            .map(PathBuf::from)
            .unwrap_or_else(|| self.root.join("target"));
        if p.is_absolute() {
            p
        } else {
            self.root.join(p)
        }
    }
    fn command(&mut self, args: &[OsString]) -> Result<Command> {
        ensure!(!args.is_empty(), "empty command");
        self.commands.push(serde_json::to_value(
            args.iter()
                .map(|v| v.to_str().context("command path must be UTF-8"))
                .collect::<Result<Vec<_>>>()?,
        )?);
        eprintln!(
            "+ {}",
            args.iter()
                .map(|a| format!("{a:?}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let mut c = Command::new(&args[0]);
        c.args(&args[1..])
            .current_dir(&self.root)
            .env_clear()
            .envs(&self.env);
        Ok(c)
    }
    fn log(&self, bytes: &[u8]) -> Result<()> {
        if let Some(d) = &self.log_dir {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(d.join("checks.log"))?
                .write_all(bytes)?;
        }
        Ok(())
    }
    pub fn status(&mut self, args: &[OsString]) -> Result<()> {
        let mut c = self.command(args)?;
        if let Some(d) = &self.log_dir {
            let f = OpenOptions::new()
                .append(true)
                .create(true)
                .open(d.join("checks.log"))?;
            c.stdout(f.try_clone()?).stderr(f);
        }
        let status = c.status().with_context(|| format!("start {:?}", args[0]))?;
        ensure!(status.success(), "command {:?} failed: {status}", args[0]);
        Ok(())
    }
    pub fn capture(&mut self, args: &[OsString]) -> Result<String> {
        let out = self.command(args)?.output()?;
        self.log(&out.stdout)?;
        self.log(&out.stderr)?;
        ensure!(
            out.status.success(),
            "command {:?} failed: {}\n{}",
            args[0],
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(String::from_utf8(out.stdout)?
            .trim_end_matches(['\r', '\n'])
            .to_owned())
    }
    pub fn bounded_capture(&mut self, args: &[OsString], timeout: f64) -> Result<String> {
        let mut scratch = Scratch::new(&env::temp_dir(), "command")?;
        #[cfg(unix)]
        let result = {
            // Record the same argv without recording request environments.
            drop(self.command(args)?);
            let strings = args
                .iter()
                .map(|s| s.to_str().map(str::to_owned).context("UTF-8 command"))
                .collect::<Result<Vec<_>>>()?;
            let values = self
                .env
                .iter()
                .map(|(k, v)| {
                    Ok((
                        k.to_str().context("UTF-8 env name")?.to_owned(),
                        v.to_str().context("UTF-8 env value")?.to_owned(),
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            let log = scratch.path().join("output");
            let report = crate::supervise::run(&crate::supervise::Request {
                argv: strings,
                cwd: Some(self.root.clone()),
                env: Some(values),
                stdout: log.clone(),
                stderr: log.clone(),
                report: None,
                timeout_seconds: timeout,
                maximum_bytes: 64 * 1024 * 1024,
                exclusive_limit: false,
                file_limit: None,
                journal: None,
                sample_rss: false,
            })?;
            let bytes = fs::read(log)?;
            self.log(&bytes)?;
            ensure!(
                report.success(),
                "bounded command failed: {:?}; cleanup {:?}",
                report.failure,
                report.cleanup_failure
            );
            String::from_utf8(bytes)
        };
        #[cfg(not(unix))]
        let result = {
            // Preserve the legacy Windows root-process timeout contract. Docker
            // resources have an independent explicit retirement/absence gate.
            let output = scratch.path().join("output");
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)?;
            let mut command = self.command(args)?;
            command.stdout(file.try_clone()?).stderr(file);
            let mut child = command.spawn()?;
            let started = std::time::Instant::now();
            let status = loop {
                if let Some(s) = child.try_wait()? {
                    break Some(s);
                }
                if started.elapsed().as_secs_f64() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            let bytes = fs::read(output)?;
            self.log(&bytes)?;
            ensure!(
                status.is_some_and(|s| s.success()),
                "bounded command failed or timed out"
            );
            String::from_utf8(bytes)
        };
        scratch.close()?;
        Ok(result?)
    }
    // Compiler environment output can contain credentials. Keep both streams
    // out of logs and errors; callers retain only explicit identities/hashes.
    pub fn capture_private(&mut self, args: &[OsString]) -> Result<Vec<u8>> {
        #[cfg(windows)]
        {
            drop(self.command(args)?);
            let timeout = self.private_deadline.map_or(120., |end| {
                end.saturating_duration_since(std::time::Instant::now())
                    .as_secs_f64()
                    .min(120.)
            });
            ensure!(timeout > 0., "private toolchain discovery deadline");
            crate::windows_process::capture_private(args, &self.root, &self.env, timeout)
        }
        #[cfg(not(windows))]
        {
            let out = self.command(args)?.output()?;
            ensure!(
                out.status.success(),
                "private toolchain command failed: {}",
                out.status
            );
            Ok(out.stdout)
        }
    }
    pub fn task(&mut self, args: impl IntoIterator<Item = OsString>) -> Result<()> {
        let mut command = vec![path(std::env::current_exe()?)];
        command.extend(args);
        self.status(&command)
    }
}

pub struct Scratch {
    path: PathBuf,
    active: bool,
}
impl Scratch {
    pub fn new(parent: &Path, label: &str) -> Result<Self> {
        fs::create_dir_all(parent)?;
        let tick = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for i in 0..100 {
            let p = parent.join(format!(
                ".weekend-{label}-{}-{tick}-{i}",
                std::process::id()
            ));
            match fs::create_dir(&p) {
                Ok(()) => {
                    return Ok(Self {
                        path: dunce::canonicalize(&p)?,
                        active: true,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }
        }
        bail!("could not create owned temporary directory")
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn close(&mut self) -> Result<()> {
        if self.active {
            fs::remove_dir_all(&self.path)?;
            self.active = false;
        }
        Ok(())
    }
    pub fn keep(&mut self) {
        self.active = false;
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.active
            && let Err(e) = self.close()
        {
            eprintln!("xtask: could not retire {}: {e}", self.path.display());
        }
    }
}

pub fn hash_tree(root: &Path) -> Result<BTreeMap<String, String>> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            let ty = entry.file_type()?;
            ensure!(!ty.is_symlink(), "symlink in package: {}", p.display());
            if ty.is_dir() {
                visit(root, &p, out)?;
            } else if ty.is_file() && p != root.join("manifest.json") {
                out.insert(
                    p.strip_prefix(root)?.to_string_lossy().into_owned(),
                    sha(&p)?,
                );
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out)?;
    Ok(out)
}

// Qualification trees include vendored crates, unlike the small delivered package.
// Prune caches BEFORE visiting them, and reject aliases/nonregular inputs.
pub(crate) fn qualification_inventory(
    root: &Path,
    skip: &[&str],
) -> Result<BTreeMap<String, String>> {
    fn walk(
        root: &Path,
        dir: &Path,
        out: &mut BTreeMap<String, String>,
        skip: &[&str],
        depth: usize,
        entries: &mut usize,
        total: &mut u64,
    ) -> Result<()> {
        ensure!(depth <= 64, "trace directory depth budget");
        let directory = fs::symlink_metadata(dir)?;
        ensure!(
            directory.is_dir()
                && !directory.file_type().is_symlink()
                && !crate::delivery::reparse(&directory),
            "trace directory alias"
        );
        for entry in fs::read_dir(dir)? {
            let file = entry?.path();
            let name = file
                .strip_prefix(root)?
                .to_str()
                .context("UTF-8 trace path")?
                .replace('\\', "/");
            if (skip.contains(&"cargo-target")
                && file.file_name().is_some_and(|n| n == "cargo-target"))
                || skip
                    .iter()
                    .any(|s| name == *s || name.starts_with(&format!("{s}/")))
            {
                continue;
            }
            *entries += 1;
            ensure!(*entries <= 100_000, "trace entry budget");
            let meta = fs::symlink_metadata(&file)?;
            ensure!(
                !meta.file_type().is_symlink() && !crate::delivery::reparse(&meta),
                "trace input alias"
            );
            if meta.is_dir() {
                walk(root, &file, out, skip, depth + 1, entries, total)?;
            } else {
                ensure!(
                    meta.is_file() && meta.len() <= 512 * 1024 * 1024,
                    "nonregular/oversized trace input"
                );
                *total = total
                    .checked_add(meta.len())
                    .context("trace byte overflow")?;
                ensure!(*total <= 8 * 1024 * 1024 * 1024, "trace byte budget");
                out.insert(name, sha(&file)?);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out, skip, 0, &mut 0, &mut 0)?;
    Ok(out)
}

pub(crate) fn cargo_build_env(
    original: &BTreeMap<OsString, OsString>,
) -> BTreeMap<OsString, OsString> {
    let mut env = original.clone();
    env.retain(|k, _| {
        let k = k.to_string_lossy();
        let k = if cfg!(windows) {
            std::borrow::Cow::Owned(k.to_ascii_uppercase())
        } else {
            k
        };
        matches!(k.as_ref(), "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN")
            || !(k.starts_with("RUST")
                || k.starts_with("CARGO_PROFILE_")
                || k.starts_with("CARGO_TARGET_")
                || k.starts_with("CARGO_BUILD_")
                || matches!(
                    k.as_ref(),
                    "CARGO_ENCODED_RUSTFLAGS"
                        | "CARGO_ENCODED_RUSTDOCFLAGS"
                        | "CARGO_INCREMENTAL"
                        | "LLVM_PROFILE_FILE"
                        | "LD_PRELOAD"
                        | "DYLD_INSERT_LIBRARIES"
                ))
    });
    env.insert("CARGO_INCREMENTAL".into(), "0".into());
    env
}

pub(crate) fn copy_qualification_tree(from: &Path, to: &Path) -> Result<()> {
    // Validate every input before creating any destination, then verify the copy.
    let before = qualification_inventory(from, &[])?;
    fn copy(from: &Path, to: &Path) -> Result<()> {
        fs::create_dir(to)?;
        for e in fs::read_dir(from)? {
            let e = e?;
            let meta = fs::symlink_metadata(e.path())?;
            if meta.is_dir() {
                copy(&e.path(), &to.join(e.file_name()))?;
            } else {
                ensure!(meta.is_file(), "nonregular trace copy input");
                fs::copy(e.path(), to.join(e.file_name()))?;
            }
        }
        Ok(())
    }
    copy(from, to)?;
    ensure!(
        before == qualification_inventory(from, &[])?
            && before == qualification_inventory(to, &[])?,
        "trace copy changed inputs"
    );
    Ok(())
}
