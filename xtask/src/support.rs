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
    process::{Command, Stdio},
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
}
impl Run {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            env: env::vars_os().collect(),
            commands: vec![],
            log_dir: None,
        }
    }
    pub fn python(&self) -> OsString {
        self.env
            .get(OsStr::new("WEEKEND_PYTHON"))
            .cloned()
            .unwrap_or_else(|| path(if cfg!(windows) { "python" } else { "python3" }))
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
    // The Windows environment response contains process secrets. Never log
    // helper stdin/stdout; only its command, stderr and selected policy results.
    pub fn helper(&mut self, action: &str, input: &Value) -> Result<Value> {
        let args = vec![
            self.python(),
            path("-I"),
            path("-B"),
            path(self.root.join("tools/release.py")),
            path("--xtask-helper"),
            path(action),
        ];
        let mut c = self.command(&args)?;
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = c.spawn()?;
        let write = serde_json::to_writer(child.stdin.take().context("helper stdin")?, input);
        let out = child.wait_with_output()?;
        self.log(&out.stderr)?;
        ensure!(
            out.status.success(),
            "package helper {action} failed: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        write?;
        serde_json::from_slice(&out.stdout).context("package helper returned invalid JSON")
    }
    pub fn leaf(&mut self, script: &str, args: impl IntoIterator<Item = OsString>) -> Result<()> {
        let mut command = vec![
            self.python(),
            path("-I"),
            path("-B"),
            path(self.root.join(script)),
        ];
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
