//! Linux-only controller for a privately owned, disposable ext4 quota mount.
use crate::{
    artifact_trace::{inventory, nonce},
    assurance::{self, Campaign},
    quota_oracle::{self, CEILING, CHUNK, LOG_BYTES, Space},
    resource_oracle::{self, MIB, number},
    supervise::{self, Request},
    support::{Run, cmd, path, sha},
    toolchain,
};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fs::{self, DirBuilder, OpenOptions},
    io::Read,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, chown},
    path::{Path, PathBuf},
    time::Duration,
};
const HOST_TOOLS: [&str; 8] = [
    "losetup",
    "mkfs.ext4",
    "mount",
    "findmnt",
    "setquota",
    "setpriv",
    "umount",
    "uname",
];
const ARRAY_MOUNT: &[&str] = &["--json", "--mountpoint"];
const MAX_FIXTURE_BYTES: u64 = 64 * MIB;
const MAX_RUNNER_BYTES: u64 = 128 * MIB;

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    pub fixture: PathBuf,
    #[arg(long)]
    pub source: String,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, conflicts_with = "maintenance")]
    pub probe_only: bool,
    #[arg(long)]
    pub maintenance: bool,
    /// Exhaust the user inode quota during maintenance, with no block quota.
    #[arg(long, requires = "maintenance")]
    pub inode_quota: bool,
    /// Exhaust all available inodes on a private ext4 filesystem with no quota.
    #[arg(long, requires = "maintenance", conflicts_with = "inode_quota")]
    pub inode_enospc: bool,
}
#[derive(Debug)]
struct Unsupported(String);
impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Unsupported {}
fn unsupported(error: anyhow::Error, context: &str) -> anyhow::Error {
    Unsupported(format!("{context}: {error:#}")).into()
}
fn valid_source(source: &str) -> bool {
    source.len() == 40
        && source
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn fresh_output(base: &Path, output: &Path) -> Result<PathBuf> {
    let output = assurance::absolute(base, output);
    ensure!(
        fs::symlink_metadata(&output).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "output already exists (including dangling symlinks)"
    );
    ensure!(
        output.parent().is_some_and(Path::is_dir),
        "output parent must already be a directory"
    );
    crate::notices::resolve(&output)
}
fn guest_env(source: &str) -> BTreeMap<OsString, OsString> {
    [
        ("PATH", "/usr/sbin:/usr/bin:/sbin:/bin"),
        ("LC_ALL", "C"),
        ("WEEKEND_DB_SOURCE_ID", source),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
fn command_context(base: &Path, source: &str) -> Result<Run> {
    let mut run = Run::new(base.to_owned());
    run.env = guest_env(source);
    run.env.insert("HOME".into(), "/root".into());
    Ok(run)
}
fn support_tools(c: &Campaign<'_>, inode_enospc: bool) -> Result<BTreeMap<String, PathBuf>> {
    HOST_TOOLS
        .into_iter()
        .filter(|&name| !inode_enospc || name != "setquota")
        .map(|name| {
            Ok((
                name.into(),
                dunce::canonicalize(
                    toolchain::which(OsStr::new(name), &c.env)
                        .ok_or_else(|| Unsupported(format!("missing quota tool: {name}")))?,
                )?,
            ))
        })
        .collect()
}
fn executable_hashes(paths: &BTreeMap<String, PathBuf>) -> Result<Value> {
    Ok(json!(
        paths
            .iter()
            .map(|(n, p)| Ok((n.clone(), json!({"path":p,"sha256":sha(p)?}))))
            .collect::<Result<BTreeMap<_, _>>>()?
    ))
}
fn qualification_files(base: &Path, maintenance: bool) -> Result<BTreeMap<String, String>> {
    let shared = [
        "Cargo.lock",
        "xtask/Cargo.toml",
        "xtask/src/main.rs",
        "xtask/src/quota_host.rs",
        "xtask/src/quota_guest.rs",
        "xtask/src/quota_oracle.rs",
        "xtask/src/supervise.rs",
        "xtask/src/assurance.rs",
        "tools/quota/fixture.rs",
    ];
    let more = maintenance.then_some([
        "xtask/src/resource_oracle.rs",
        "tools/resource-envelope/Cargo.toml",
        "tools/resource-envelope/Cargo.lock",
        "tools/resource-envelope/src/compound.rs",
        "tools/resource-envelope/src/resilience.rs",
        "tools/resource-envelope/src/space.rs",
    ]);
    shared
        .into_iter()
        .chain(more.into_iter().flatten())
        .map(|name| Ok((name.into(), sha(&base.join(name))?)))
        .collect()
}
fn recheck_qualification_files(
    root: &Path,
    maintenance: bool,
    before: &BTreeMap<String, String>,
) -> Result<()> {
    ensure!(
        qualification_files(root, maintenance)? == *before,
        "qualification tool inventory changed"
    );
    Ok(())
}
fn compare_tree(root: &Path, hashes: &BTreeMap<String, String>) -> Result<()> {
    for (name, digest) in hashes {
        let file = root.join(name);
        let meta = fs::symlink_metadata(&file)?;
        ensure!(
            meta.is_file()
                && dunce::canonicalize(&file)?.starts_with(root)
                && sha(&file)? == *digest,
            "source differs from requested commit: {name}"
        );
    }
    Ok(())
}
fn source_binding(c: &mut Campaign<'_>, source: &str) -> Result<BTreeMap<String, String>> {
    let git = vec![
        "git".into(),
        "-c".into(),
        format!("safe.directory={}", c.run.root.display()).into(),
        "-C".into(),
        path(&c.run.root),
    ];
    let mut args = git.clone();
    args.extend(cmd([
        "rev-parse",
        "--verify",
        "--end-of-options",
        &format!("{source}^{{commit}}"),
    ]));
    ensure!(
        c.command(args, "source-commit", None, 30.)? == source,
        "source must be the exact commit object"
    );
    let mut args = git.clone();
    args.extend(cmd(["ls-tree", "-r", source]));
    let entries = c.command(args, "source-shape", None, 30.)?;
    ensure!(
        !entries.is_empty()
            && entries
                .lines()
                .all(|r| r.starts_with("100644 blob ") || r.starts_with("100755 blob ")),
        "nonregular archived source"
    );
    let tar = c.output.join("source.tar");
    let tree = c.output.join("frozen-source");
    fs::create_dir(&tree)?;
    let mut args = git;
    args.extend([
        "archive".into(),
        "--format=tar".into(),
        "--output".into(),
        path(&tar),
        source.into(),
    ]);
    c.command(args, "source-archive", None, 30.)?;
    c.command(
        vec![
            "tar".into(),
            "-xf".into(),
            path(&tar),
            "-C".into(),
            path(&tree),
        ],
        "extract-source",
        None,
        30.,
    )?;
    let files = inventory(&tree, &[])?;
    compare_tree(&c.run.root, &files)?;
    c.evidence["source_binding"] =
        json!({"source":source,"commit_verified":true,"archive_sha256":sha(&tar)?,"files":files});
    Ok(files)
}
#[derive(Debug)]
struct Scratch {
    directory: PathBuf,
    image: PathBuf,
    mount: PathBuf,
    image_identity: Option<(u64, u64)>,
}
impl Scratch {
    fn create() -> Result<Self> {
        let directory =
            Path::new("/tmp").join(format!("weekend-quota-{}-{}", nonce()?, std::process::id()));
        DirBuilder::new().mode(0o711).create(&directory)?;
        Ok(Self {
            image: directory.join("image.ext4"),
            mount: directory.join("mount"),
            directory,
            image_identity: None,
        })
    }
    fn prepare(&mut self, bytes: u64) -> Result<()> {
        fs::create_dir(&self.mount)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&self.image)?;
        let info = file.metadata()?;
        self.image_identity = Some((info.dev(), info.ino()));
        file.set_len(bytes)?;
        Ok(())
    }
    fn ownership(&self) -> Result<()> {
        let meta = fs::symlink_metadata(&self.directory)?;
        let name = self
            .directory
            .file_name()
            .and_then(|s| s.to_str())
            .context("scratch name")?;
        let valid = name
            .strip_prefix("weekend-quota-")
            .and_then(|s| s.split_once('-'))
            .is_some_and(|(nonce, pid)| {
                nonce.len() == 32
                    && nonce.bytes().all(|b| b.is_ascii_hexdigit())
                    && !pid.is_empty()
                    && pid.bytes().all(|b| b.is_ascii_digit())
            });
        // SAFETY: read-only query of the current credentials.
        ensure!(
            self.directory.is_absolute()
                && valid
                && meta.is_dir()
                && meta.uid() == unsafe { libc::geteuid() },
            "unowned quota scratch directory"
        );
        ensure!(
            self.image == self.directory.join("image.ext4")
                && self.mount == self.directory.join("mount"),
            "unowned quota paths"
        );
        if let Some(identity) = self.image_identity {
            let meta = fs::symlink_metadata(&self.image)?;
            ensure!(
                meta.is_file() && (meta.dev(), meta.ino()) == identity,
                "quota backing image identity changed"
            );
        }
        Ok(())
    }
}
struct Calls<'a, 'b> {
    c: &'a mut Campaign<'b>,
    tools: BTreeMap<String, PathBuf>,
    sequence: usize,
}
struct Output {
    stdout: String,
    stderr: String,
    pid: u32,
    exit: i32,
}
impl Calls<'_, '_> {
    fn command(
        &mut self,
        argv: Vec<OsString>,
        seconds: f64,
        allowed: &[i32],
        env: Option<&BTreeMap<OsString, OsString>>,
        cleanup: bool,
    ) -> Result<Output> {
        if !cleanup {
            ensure!(!self.c.cancelled(), "quota controller cancelled");
        }
        self.sequence += 1;
        let label = format!(
            "{}-{}",
            if cleanup { "cleanup" } else { "quota" },
            self.sequence
        );
        let mut argv = argv;
        if let Some(tool) = argv
            .first()
            .and_then(|name| name.to_str())
            .and_then(|name| self.tools.get(name))
        {
            argv[0] = path(tool);
        }
        let argv = argv
            .into_iter()
            .map(|p| {
                p.into_string()
                    .map_err(|_| anyhow::anyhow!("UTF-8 quota command"))
            })
            .collect::<Result<Vec<_>>>()?;
        let env = env
            .unwrap_or(&self.c.env)
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.to_str().context("UTF-8 env key")?.to_owned(),
                    v.to_str().context("UTF-8 env value")?.to_owned(),
                ))
            })
            .collect::<Result<_>>()?;
        let stdout = self.c.output.join(format!("{label}.stdout"));
        let stderr = self.c.output.join(format!("{label}.stderr"));
        let result = supervise::run(&Request {
            argv: argv.clone(),
            cwd: Some(self.c.run.root.clone()),
            env: Some(env),
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            report: Some(self.c.output.join(format!("{label}.process.json"))),
            timeout_seconds: seconds,
            maximum_bytes: LOG_BYTES,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        });
        let report = match result {
            Ok(report) => report,
            Err(e) => {
                self.c.evidence["commands"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"label":label,"argv":argv,"error":format!("{e:#}")}));
                return Err(e);
            }
        };
        self.c.evidence["commands"]
            .as_array_mut()
            .unwrap()
            .push(json!({"label":label,"argv":argv,"supervision":report}));
        if !cleanup {
            self.c.save()?;
        }
        let exit = report.returncode.context("quota child did not exit")?;
        ensure!(
            !report.timed_out
                && !report.cancelled
                && report.cleanup_verified
                && report.cleanup_failure.is_none()
                && (report.failure.is_none()
                    || report.failure.as_deref() == Some(&format!("child exit Some({exit})")))
                && allowed.contains(&exit),
            "{label} failed: {:?}",
            report.failure
        );
        Ok(Output {
            stdout: fs::read_to_string(stdout)?,
            stderr: fs::read_to_string(stderr)?,
            pid: report.pid.context("quota child PID missing")?,
            exit,
        })
    }
    fn exec(&mut self, argv: Vec<OsString>) -> Result<Output> {
        self.command(argv, 30., &[0], None, false)
    }
    fn guest(
        &mut self,
        args: Vec<OsString>,
        env: &BTreeMap<OsString, OsString>,
        seconds: f64,
        allowed: &[i32],
    ) -> Result<Output> {
        self.command(args, seconds, allowed, Some(env), false)
    }
    fn loops(&mut self, s: &Scratch, cleanup: bool) -> Result<Vec<String>> {
        let o = self.command(
            cmd(["losetup", "--json", "--list", "--output", "NAME,BACK-FILE"]),
            if cleanup { 10. } else { 30. },
            &[0],
            None,
            cleanup,
        )?;
        quota_oracle::owned_loops(&o.stdout, &s.image)
    }
    fn mount(&mut self, s: &Scratch, cleanup: bool) -> Result<Option<String>> {
        let mut args = vec!["findmnt".into()];
        args.extend(cmd(ARRAY_MOUNT));
        args.push(path(&s.mount));
        args.extend(cmd(["--output", "SOURCE,FSTYPE,TARGET,OPTIONS"]));
        mount_state(self.command(
            args,
            if cleanup { 10. } else { 30. },
            &[0, 1],
            None,
            cleanup,
        )?)
    }
    fn verify_mount(&mut self, s: &Scratch, device: &str) -> Result<Value> {
        s.ownership()?;
        ensure!(
            self.loops(s, false)? == [device],
            "attached loop identity mismatch"
        );
        quota_oracle::exact_mount(
            &self.mount(s, false)?.context("private mount absent")?,
            &s.mount,
            device,
        )
    }
}
fn mount_state(o: Output) -> Result<Option<String>> {
    if o.exit == 1 {
        ensure!(
            o.stdout.trim().is_empty() && o.stderr.trim().is_empty(),
            "mount absence query errored"
        );
        Ok(None)
    } else {
        ensure!(
            o.exit == 0 && !o.stdout.trim().is_empty(),
            "ambiguous mount absence"
        );
        Ok(Some(o.stdout))
    }
}
#[derive(Debug, PartialEq, Eq)]
enum CleanupOp {
    LoopInventory,
    Mount,
    Unmount,
    Detach(String),
}
fn cleanup(c: &mut Calls<'_, '_>, s: &Scratch) -> Result<Value> {
    cleanup_with(
        s,
        |op| match op {
            CleanupOp::Mount => c.mount(s, true),
            CleanupOp::LoopInventory => Ok(Some(
                c.command(
                    cmd(["losetup", "--json", "--list", "--output", "NAME,BACK-FILE"]),
                    10.,
                    &[0],
                    None,
                    true,
                )?
                .stdout,
            )),
            CleanupOp::Unmount => {
                c.command(vec!["umount".into(), path(&s.mount)], 10., &[0], None, true)?;
                Ok(None)
            }
            CleanupOp::Detach(device) => {
                c.command(cmd(["losetup", "--detach", &device]), 10., &[0], None, true)?;
                Ok(None)
            }
        },
        || std::thread::sleep(Duration::from_millis(100)),
    )
}
fn cleanup_with(
    s: &Scratch,
    mut call: impl FnMut(CleanupOp) -> Result<Option<String>>,
    mut pause: impl FnMut(),
) -> Result<Value> {
    fn loops(
        s: &Scratch,
        call: &mut impl FnMut(CleanupOp) -> Result<Option<String>>,
    ) -> Result<Vec<String>> {
        quota_oracle::owned_loops(
            &call(CleanupOp::LoopInventory)?.context("missing loop inventory")?,
            &s.image,
        )
    }
    s.ownership()?;
    let mut errors = vec![];
    let mut drift = false;
    let unmount = (|| -> Result<()> {
        let devices = loops(s, &mut call)?;
        if let Some(mount) = call(CleanupOp::Mount)? {
            if devices.len() != 1
                || quota_oracle::exact_mount(&mount, &s.mount, &devices[0]).is_err()
            {
                drift = true;
                anyhow::bail!("mount identity drift");
            }
            call(CleanupOp::Unmount)?;
        }
        Ok(())
    })();
    if let Err(e) = unmount {
        errors.push(format!("{e:#}"));
    }
    let detach = (|| -> Result<()> {
        let devices = loops(s, &mut call)?;
        ensure!(!drift, "mount identity drift prevents detach");
        for dev in devices {
            call(CleanupOp::Detach(dev))?;
        }
        Ok(())
    })();
    if let Err(e) = detach {
        errors.push(format!("{e:#}"));
    }
    let (mut mount_absent, mut loops_absent) = (false, false);
    for _ in 0..10 {
        let mount = call(CleanupOp::Mount);
        let loop_devices = loops(s, &mut call);
        match (mount, loop_devices) {
            (Ok(mount), Ok(devs)) => {
                mount_absent = mount.is_none();
                loops_absent = devs.is_empty();
                if mount_absent && loops_absent {
                    break;
                }
            }
            (m, l) => {
                errors.push(format!("absence queries: {m:?}, {l:?}"));
                break;
            }
        }
        pause();
    }
    let verified = mount_absent && loops_absent;
    if verified {
        s.ownership()?;
        fs::remove_dir_all(&s.directory)?;
    }
    Ok(
        json!({"errors":errors,"mount_absent":mount_absent,"loops_absent":loops_absent,"verified":verified}),
    )
}
fn private_directory(path: &Path, uid: u32) -> Result<()> {
    DirBuilder::new().mode(0o700).create(path)?;
    chown(path, Some(uid), Some(uid))?;
    Ok(())
}
fn guest_uid() -> Result<u32> {
    for uid in 60000..65000 {
        let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut bytes = [0u8; 65536];
        let mut result = std::ptr::null_mut();
        // SAFETY: getpwuid_r initializes the bounded writable buffers/result.
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                pwd.as_mut_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                &mut result,
            )
        };
        ensure!(status == 0, "numeric guest account lookup failed: {status}");
        if result.is_null() {
            return Ok(uid);
        }
    }
    anyhow::bail!("no unused private numeric UID")
}
fn drop_privileges(uid: u32) -> Vec<OsString> {
    let uid = uid.to_string();
    cmd([
        "setpriv",
        "--reuid",
        &uid,
        "--regid",
        &uid,
        "--clear-groups",
        "--inh-caps=-all",
        "--ambient-caps=-all",
        "--bounding-set=-all",
        "--no-new-privs",
    ])
}
struct Guest {
    uid: u32,
    runner: PathBuf,
    fixture: PathBuf,
    data: PathBuf,
    env: BTreeMap<OsString, OsString>,
    drop: Vec<OsString>,
}
impl Guest {
    fn data_names(&self) -> Result<BTreeSet<OsString>> {
        Ok(fs::read_dir(&self.data)?
            .map(|r| r.map(|e| e.file_name()))
            .collect::<std::io::Result<_>>()?)
    }

    fn probe(
        &self,
        c: &mut Calls<'_, '_>,
        mode: &str,
        ceiling: u64,
        space: Space,
        seed: Option<&Path>,
    ) -> Result<Value> {
        let mut args = self.drop.clone();
        args.extend([
            path(&self.runner),
            "quota-guest".into(),
            mode.into(),
            path(&self.data),
            self.uid.to_string().into(),
            "--ceiling".into(),
            ceiling.to_string().into(),
        ]);
        if let Some(seed) = seed {
            args.extend(["--seed".into(), path(seed)]);
        }
        let result = c.guest(args, &self.env, 120., &[0])?;
        let row = quota_oracle::one_json(&result.stdout)?;
        quota_oracle::verify_probe(&row, self.uid, mode, ceiling, space)?;
        ensure!(row["pid"] == result.pid, "guest probe PID substitution");
        Ok(row)
    }
    fn fixture(
        &self,
        c: &mut Calls<'_, '_>,
        args: Vec<OsString>,
        timeout: f64,
        allowed: &[i32],
    ) -> Result<Output> {
        let mut argv = self.drop.clone();
        argv.push(path(&self.fixture));
        argv.extend(args);
        c.guest(argv, &self.env, timeout, allowed)
    }
    fn normal(&self, c: &mut Calls<'_, '_>, source: &str, probe_only: bool) -> Result<()> {
        c.c.evidence["calibration_full"] = self.probe(c, "fill", CHUNK, Space::Quota, None)?;
        self.probe(c, "remove", CHUNK, Space::Quota, None)?;
        c.c.evidence["calibration_control"] =
            self.probe(c, "write-control", CHUNK, Space::Quota, None)?;
        if probe_only {
            return Ok(());
        }
        let db = self.data.join("database.wdb");
        let mut results = vec![];
        let mut pids = BTreeSet::new();
        for (stage, revision) in [
            ("seed", 3),
            ("fail", 3),
            ("inspect", 3),
            ("retry", 4),
            ("inspect", 4),
        ] {
            let mut args = vec![stage.into(), path(&db)];
            if stage == "inspect" {
                args.push(revision.to_string().into());
            }
            let result = self.fixture(c, args, 30., &[0])?;
            let row = quota_oracle::one_json(&result.stdout)?;
            quota_oracle::fixture(&row, source, stage, revision, result.pid)?;
            ensure!(pids.insert(result.pid), "fixture reused a process");
            results.push(row);
            if stage == "seed" {
                c.c.evidence["engine_full"] = self.probe(c, "fill", CHUNK, Space::Quota, None)?;
            }
            if stage == "inspect" && revision == 3 {
                self.probe(c, "remove", CHUNK, Space::Quota, None)?;
            }
        }
        c.c.evidence["results"] = json!(results);
        Ok(())
    }
}
fn bounded_executable(file: &Path, ceiling: u64) -> Result<()> {
    ensure!(file.is_absolute(), "absolute executable path required");
    let info = fs::symlink_metadata(file)?;
    ensure!(
        info.is_file() && info.len() > 0 && info.len() <= ceiling,
        "{} must be a regular executable between 1 and {ceiling} bytes (observed {})",
        file.display(),
        info.len()
    );
    Ok(())
}
fn copy_executable(from: &Path, to: &Path, ceiling: u64) -> Result<String> {
    bounded_executable(from, ceiling)?;
    let digest = sha(from)?;
    let input = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(from)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o500)
        .custom_flags(libc::O_NOFOLLOW)
        .open(to)?;
    let size = std::io::copy(&mut input.take(ceiling + 1), &mut output)?;
    ensure!(
        size > 0 && size <= ceiling,
        "executable changed size while staging"
    );
    drop(output);
    fs::set_permissions(to, fs::Permissions::from_mode(0o555))?;
    ensure!(
        sha(to)? == digest && sha(from)? == digest,
        "executable changed while staging"
    );
    Ok(digest)
}
pub fn run(run: &Run, opts: Options) -> Result<()> {
    let output = fresh_output(&run.root, &opts.output)?;
    ensure!(valid_source(&opts.source), "full source SHA required");
    ensure!(
        !(opts.maintenance && opts.probe_only),
        "maintenance cannot be capability-only"
    );
    // SAFETY: read-only query. The guest subcommand independently requires all
    // four UID/GID slots, no groups, no capabilities and no_new_privs.
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "requires Linux root controller; elevate only this entrypoint"
    );
    let run = command_context(&run.root, &opts.source)?;
    let description = if opts.inode_enospc {
        "native global inode exhaustion on owned private Linux ext4"
    } else {
        "native kernel quotas on owned private Linux ext4"
    };
    let mut campaign = Campaign::detached(&run, &output, description)?;
    fs::set_permissions(&campaign.output, fs::Permissions::from_mode(0o700))?;
    campaign.evidence["verified"] = json!(false);
    campaign.evidence["status"] = json!("running");
    campaign.save()?;
    let mut scratch = None;
    let mut call = Calls {
        c: &mut campaign,
        tools: BTreeMap::new(),
        sequence: 0,
    };
    let result = (|| -> Result<()> {
        bounded_executable(&opts.fixture, MAX_FIXTURE_BYTES)?;
        let source_files = source_binding(call.c, &opts.source)?;
        let tools = qualification_files(&run.root, opts.maintenance)?;
        call.c.evidence["tools_sha256"] = json!(tools);
        call.tools = support_tools(call.c, opts.inode_enospc)?;
        let hashes = executable_hashes(&call.tools)?;
        call.c.evidence["executables"] = hashes.clone();
        scratch = Some(Scratch::create()?);
        let s = scratch.as_mut().unwrap();
        let bytes = if opts.maintenance {
            512 * MIB
        } else {
            64 * MIB
        };
        s.prepare(bytes)?;
        call.c.evidence["scratch"] = json!(s.directory);
        call.c.evidence["image_bytes"] = json!(bytes);
        call.c.evidence["image_identity"] = json!(s.image_identity);
        let kernel = call.exec(cmd(["uname", "-a"]))?;
        call.c.evidence["kernel"] = json!(kernel.stdout);
        let (hard, inodes, space) = if opts.inode_enospc {
            (0, 0, Space::InodeEnospc)
        } else if opts.inode_quota {
            // The guest owns the directory, source, backup and one ballast inode.
            (0, 4, Space::InodeQuota)
        } else if opts.maintenance {
            (256 * 1024, 0, Space::Quota)
        } else {
            (4096, 0, Space::Quota)
        };
        let device = (|| -> Result<String> {
            call.exec(quota_oracle::formatter(
                &call.tools["mkfs.ext4"],
                &s.image,
                space,
            ))?;
            let dev = call
                .exec(vec![
                    "losetup".into(),
                    "--find".into(),
                    "--show".into(),
                    "--nooverlap".into(),
                    path(&s.image),
                ])?
                .stdout
                .trim()
                .to_owned();
            ensure!(
                call.loops(s, false)? == [dev.as_str()],
                "attached loop identity mismatch"
            );
            call.exec(vec![
                "mount".into(),
                "-t".into(),
                "ext4".into(),
                "-o".into(),
                if opts.inode_enospc {
                    "rw,nodev,nosuid,noexec".into()
                } else {
                    "rw,usrquota,nodev,nosuid,noexec".into()
                },
                dev.clone().into(),
                path(&s.mount),
            ])?;
            call.verify_mount(s, &dev)?;
            Ok(dev)
        })()
        .map_err(|e| unsupported(e, "private quota filesystem unavailable"))?;
        let uid = guest_uid()?;
        let data = s.mount.join("data");
        private_directory(&data, uid)?;
        let mount = call.verify_mount(s, &device)?;
        if opts.inode_enospc {
            let options = mount["options"]
                .as_str()
                .context("missing ext4 mount options")?;
            ensure!(
                !options.split(',').any(|option| option.contains("quota")),
                "global inode exhaustion requires an ext4 mount without quotas"
            );
            call.c.evidence["inode_mount"] = mount;
            private_directory(&s.mount.join("inode-ballast"), uid)?;
            call.c.evidence["inode_limit"] = json!({"uid":uid,"gid":uid,"type":"filesystem","requested_inodes":quota_oracle::EXT4_INODES,"quota_enabled":false});
        } else {
            call.c.evidence["quota"] = json!({"uid":uid,"gid":uid,"type":"user","soft_kib":hard,"hard_kib":hard,"inode_soft":inodes,"inode_hard":inodes,"image_bytes":bytes});
            let id = uid.to_string();
            let cap = hard.to_string();
            let inode_cap = inodes.to_string();
            call.exec(cmd([
                OsStr::new("setquota"),
                OsStr::new("-u"),
                OsStr::new(&id),
                OsStr::new(&cap),
                OsStr::new(&cap),
                OsStr::new(&inode_cap),
                OsStr::new(&inode_cap),
                s.mount.as_os_str(),
            ]))
            .map_err(|e| unsupported(e, "private quota configuration unavailable"))?;
        }
        let runner = s.directory.join("xtask-guest");
        let current = dunce::canonicalize(std::env::current_exe()?)?;
        let runner_hash = copy_executable(&current, &runner, MAX_RUNNER_BYTES)?;
        let fixture = s.directory.join("fixture");
        let fixture_hash = copy_executable(&opts.fixture, &fixture, MAX_FIXTURE_BYTES)?;
        let mut guest = Guest {
            runner,
            fixture,
            data,
            uid,
            env: guest_env(&opts.source),
            drop: drop_privileges(uid),
        };
        call.c.evidence["guest_runner_sha256"] = json!(runner_hash);
        call.c.evidence["fixture_sha256"] = json!(fixture_hash);
        call.c.evidence["guest_identity"] =
            guest.probe(&mut call, "identity", CHUNK, space, None)?;
        if opts.maintenance {
            let seed = s.directory.join("seed-parent");
            private_directory(&seed, uid)?;
            guest.maintenance(&mut call, &opts.source, space, &seed.join("seed"))?;
        } else {
            guest.normal(&mut call, &opts.source, opts.probe_only)?;
        }
        compare_tree(&run.root, &source_files)?;
        recheck_qualification_files(&run.root, opts.maintenance, &tools)?;
        ensure!(
            sha(&output.join("source.tar"))? == call.c.evidence["source_binding"]["archive_sha256"],
            "source archive changed"
        );
        ensure!(
            executable_hashes(&call.tools)? == hashes
                && sha(&current)? == runner_hash
                && sha(&guest.runner)? == runner_hash
                && sha(&opts.fixture)? == fixture_hash
                && sha(&guest.fixture)? == fixture_hash,
            "executables changed during qualification"
        );
        Ok(())
    })();
    let cleanup_result = scratch.as_ref().map(|s| cleanup(&mut call, s));
    let cleanup_verified = match cleanup_result {
        None => true,
        Some(Ok(proof)) => {
            let clean = proof["verified"] == true;
            call.c.evidence["cleanup"] = proof;
            clean
        }
        Some(Err(e)) => {
            call.c.evidence["cleanup"] = json!({"verified":false,"error":format!("{e:#}")});
            false
        }
    };
    finish_qualification(campaign, result, cleanup_verified, opts.probe_only)
}

fn finish_qualification(
    mut campaign: Campaign<'_>,
    result: Result<()>,
    cleanup_verified: bool,
    probe_only: bool,
) -> Result<()> {
    let result = if campaign.cancelled() {
        Err(anyhow::anyhow!("quota controller cancelled"))
    } else {
        result
    };
    let status = if !cleanup_verified {
        "cleanup-failed"
    } else if result.as_ref().is_err_and(|e| e.is::<Unsupported>()) {
        "unsupported"
    } else if result.is_err() {
        "failed"
    } else if probe_only {
        "capability-only"
    } else {
        "qualified"
    };
    campaign.evidence["status"] = json!(status);
    campaign.evidence["verified"] = json!(status == "qualified");
    let mut finished = if !cleanup_verified {
        Err(anyhow::anyhow!("owned quota resources remain"))
    } else if probe_only && status == "unsupported" {
        campaign.evidence["unsupported_reason"] = json!(format!("{:#}", result.unwrap_err()));
        Ok(())
    } else {
        result
    };
    // Keep bounded logs and synthetic final images; exclude the immutable source
    // copy which is bound independently by source_binding.
    match inventory(&campaign.output, &["evidence.json", "frozen-source"]) {
        Ok(hashes) => campaign.evidence["artifact_sha256"] = json!(hashes),
        Err(e) => {
            campaign.evidence["inventory_error"] = json!(format!("{e:#}"));
            if finished.is_ok() {
                finished = Err(e);
            }
        }
    }
    campaign.finish(finished)
}

fn json_rows(s: &str) -> Result<Vec<Value>> {
    s.lines().map(quota_oracle::one_json).collect()
}
struct Maintenance<'a> {
    guest: &'a Guest,
    source: &'a str,
    space: Space,
    pids: BTreeSet<u32>,
    records: Vec<Value>,
}
impl Maintenance<'_> {
    fn phase(
        &mut self,
        c: &mut Calls<'_, '_>,
        mode: &str,
        label: &str,
        image: &Path,
        changed: bool,
    ) -> Result<Value> {
        let mut args = vec![
            path(&self.guest.data),
            path(image),
            mode.into(),
            label.into(),
        ];
        if changed {
            args.push("changed".into());
        }
        let reply = self.guest.fixture(c, args, 120., &[0])?;
        let rows = json_rows(&reply.stdout)?;
        let row =
            quota_oracle::maintenance(&rows, mode, label, self.source, self.space, reply.pid)?
                .clone();
        ensure!(
            self.pids.insert(reply.pid),
            "fixture phases reused a process"
        );
        self.records
            .push(json!({"mode":mode,"label":label,"pid":reply.pid,"rows":rows}));
        Ok(row)
    }
}
impl Guest {
    fn maintenance(
        &mut self,
        c: &mut Calls<'_, '_>,
        source: &str,
        space: Space,
        seed: &Path,
    ) -> Result<()> {
        self.env
            .insert("WEEKEND_PRIVATE_MAINTENANCE".into(), space.profile().into());
        let mut data = Maintenance {
            guest: self,
            source,
            space,
            pids: BTreeSet::new(),
            records: vec![],
        };
        let original = self.data.join("source.wdb");
        let backup = self.data.join("backup.wdb");
        let reply = self.fixture(c, vec![path(seed)], 180., &[0])?;
        let seed_rows = json_rows(&reply.stdout)?;
        resource_oracle::compound(&seed_rows, "native", source)?;
        data.pids.insert(reply.pid);
        let expected=["source.wdb","backup.wdb"].into_iter().map(|name|Ok((name,json!({"bytes":fs::metadata(seed.join(name))?.len(),"sha256":sha(&seed.join(name))?})))).collect::<Result<BTreeMap<_,_>>>()?;
        let installed = self.probe(c, "install-seed", CEILING, space, Some(seed))?;
        ensure!(
            installed["installed"] == json!(expected),
            "installed seed differs"
        );
        ensure!(
            sha(&original)? == expected["source.wdb"]["sha256"].as_str().unwrap()
                && sha(&backup)? == expected["backup.wdb"]["sha256"].as_str().unwrap(),
            "controller installed hash mismatch"
        );
        c.c.evidence["maintenance_seed"] = json!({"rows":seed_rows,"installed":expected});
        data.phase(c, "space-check", "initial-source", &original, false)?;
        data.phase(c, "space-check", "initial-backup", &backup, false)?;
        let negative = self.fixture(
            c,
            vec![
                path(&self.data),
                path(&original),
                "space-fail".into(),
                "backup".into(),
            ],
            120.,
            &[101],
        )?;
        ensure!(
            data.pids.insert(negative.pid),
            "fixture phases reused a process"
        );
        ensure!(
            negative.stderr.contains("Result::unwrap_err()")
                && negative.stderr.contains("`Ok`")
                && negative.stderr.contains("CompactionStats"),
            "negative control failed at wrong assertion"
        );
        let path = self.data.join("failed-backup.wdb");
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file()
                && meta.uid() == self.uid
                && meta.len() == 63_074_304
                && sha(&path)? == sha(&backup)?,
            "non-full negative copy not actually successful"
        );
        data.phase(c, "space-check", "negative-output", &path, false)?;
        c.c.evidence["maintenance_negative"] = json!({"exit":negative.exit,"pid":negative.pid,"required_failure_rejected":true,"successful_copy_verified":true});
        fs::remove_file(path)?;
        let mut cases = vec![];
        for op in ["backup", "restore", "compact"] {
            let a = sha(&original)?;
            let b = sha(&backup)?;
            let (full, headroom) = match space {
                Space::InodeQuota => (
                    self.probe(c, "inode-fill", CEILING, space, None)?,
                    Value::Null,
                ),
                Space::InodeEnospc => (
                    self.probe(c, "inode-exhaust", CEILING, space, None)?,
                    Value::Null,
                ),
                Space::Quota | Space::Enospc => (
                    self.probe(c, "fill", CEILING, space, None)?,
                    self.probe(c, "headroom", CEILING, space, None)?,
                ),
            };
            if space == Space::InodeEnospc {
                let created = number(&full, "created")?;
                ensure!(
                    (created > 1 && op == "backup") || (created == 1 && op != "backup"),
                    "initial inode fill or one-inode refill not proven"
                );
            }
            let fail = data.phase(c, "space-fail", op, &original, false)?;
            if space.fails_at_create() {
                let expected: BTreeSet<_> = if space == Space::InodeQuota {
                    ["backup.wdb", "ballast", "source.wdb"]
                        .into_iter()
                        .map(OsString::from)
                        .collect()
                } else {
                    ["backup.wdb", "source.wdb"]
                        .into_iter()
                        .map(OsString::from)
                        .collect()
                };
                ensure!(
                    self.data_names()? == expected,
                    "inode refusal left an unexpected directory entry"
                );
            }
            ensure!(
                sha(&original)? == a && sha(&backup)? == b,
                "independent failure source hash changed"
            );
            data.phase(
                c,
                "space-check",
                &format!("full-source-{op}"),
                &original,
                false,
            )?;
            data.phase(
                c,
                "space-check",
                &format!("full-backup-{op}"),
                &backup,
                false,
            )?;
            let release = if space == Space::InodeEnospc {
                "inode-release"
            } else {
                "remove"
            };
            let remove = self.probe(c, release, CEILING, space, None)?;
            let control = self.probe(c, "write-control", CEILING, space, None)?;
            data.phase(c, "space-retry", op, &original, false)?;
            data.phase(
                c,
                "space-check",
                &format!("retried-source-{op}"),
                &original,
                false,
            )?;
            let retried = if op == "compact" {
                original.clone()
            } else {
                let prefix = if space.fails_at_create() {
                    "failed"
                } else {
                    "retry"
                };
                self.data.join(format!("{prefix}-{op}.wdb"))
            };
            ensure!(
                sha(&retried)? == b && sha(&backup)? == b,
                "retry/backup bytes differ"
            );
            data.phase(
                c,
                "space-check",
                &format!("retried-output-{op}"),
                &retried,
                false,
            )?;
            data.phase(
                c,
                "space-check",
                &format!("unchanged-backup-{op}"),
                &backup,
                false,
            )?;
            data.phase(c, "space-retire", op, &original, false)?;
            cases.push(json!({"operation":op,"full":full,"headroom":headroom,"failure":fail,"ballast_removed":remove,"write_control":control}));
        }
        data.phase(c, "space-advance", "advanced", &original, false)?;
        data.phase(c, "space-check", "advanced-source", &original, true)?;
        data.phase(c, "space-check", "final-backup", &backup, false)?;
        ensure!(
            self.data_names()? == ["backup.wdb", "source.wdb"].map(OsString::from).into(),
            "unretired outputs or ballast"
        );
        let images = c.c.output.join("images");
        fs::create_dir(&images)?;
        for image in [&original, &backup] {
            let target = images.join(image.file_name().unwrap());
            fs::copy(image, &target)?;
            ensure!(
                sha(image)? == sha(&target)?,
                "copied maintenance image changed"
            );
        }
        c.c.evidence["maintenance_cases"] = json!(cases);
        c.c.evidence["maintenance_records"] = json!(data.records);
        c.c.evidence["maintenance_pids"] = json!(data.pids);
        Ok(())
    }
}

/// Native worker for the companion ENOSPC drill. It refuses to write ballast
/// unless the runtime path is the provisioned, exact-sized disposable tmpfs.
pub fn enospc_worker(run: &Run, source: &Path, revision: &str, output: &Path) -> Result<()> {
    ensure!(valid_source(revision), "exact engine SHA required");
    // SAFETY: read-only credential query; execution drops privileges below.
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "ENOSPC worker requires container root"
    );
    let mut c = Campaign::detached(run, output, "private 256 MiB tmpfs maintenance ENOSPC")?;
    c.evidence["verified"] = json!(false);
    c.evidence["engine_source"] = json!(revision);
    c.save()?;
    let result = (|| -> Result<()> {
        let frozen = inventory(source, &[])?;
        c.evidence["engine_source_hashes"] = json!(frozen);
        let volume = Path::new("/faults");
        let filesystem = c.command(
            cmd([
                "findmnt",
                "--mountpoint",
                "/faults",
                "-n",
                "-o",
                "FSTYPE,SIZE,OPTIONS",
            ]),
            "tmpfs",
            None,
            30.,
        )?;
        ensure!(
            filesystem.split_whitespace().next() == Some("tmpfs")
                && crate::quota_guest::space(volume)?.0 == 256 * MIB,
            "private 256 MiB tmpfs required"
        );
        c.evidence["filesystem"] = json!(filesystem);
        let version = c.command(cmd(["rustc", "-vV"]), "rustc", None, 30.)?;
        c.evidence["rustc"] = json!(version);
        c.env.insert("WEEKEND_DB_SOURCE_ID".into(), revision.into());
        let output = c.output.clone();
        let target = output.join("cargo-target");
        c.command_at(
            vec![
                "cargo".into(),
                "build".into(),
                "--offline".into(),
                "--locked".into(),
                "--release".into(),
                "--manifest-path".into(),
                path(source.join("tools/resource-envelope/Cargo.toml")),
                "--features".into(),
                "compound".into(),
                "--bin".into(),
                "weekend-compound-resource".into(),
                "--target-dir".into(),
                path(&target),
            ],
            "fixture-build",
            None,
            180.,
            source,
        )?;
        let root = output.join("guest");
        DirBuilder::new().mode(0o711).create(&root)?;
        let uid = guest_uid()?;
        let data = volume.join("data");
        private_directory(&data, uid)?;
        let seed = root.join("seed-parent");
        private_directory(&seed, uid)?;
        let runner = root.join("xtask-guest");
        let current = dunce::canonicalize(std::env::current_exe()?)?;
        let runner_hash = copy_executable(&current, &runner, MAX_RUNNER_BYTES)?;
        let fixture = root.join("fixture");
        let fixture_hash = copy_executable(
            &target.join("release/weekend-compound-resource"),
            &fixture,
            MAX_FIXTURE_BYTES,
        )?;
        let tools = ["setpriv"]
            .into_iter()
            .map(|name| {
                Ok((
                    name.to_owned(),
                    dunce::canonicalize(
                        toolchain::which(OsStr::new(name), &c.env)
                            .context("setpriv unavailable")?,
                    )?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let hashes = executable_hashes(&tools)?;
        c.evidence["guest_runner_sha256"] = json!(runner_hash);
        c.evidence["fixture_sha256"] = json!(fixture_hash);
        c.evidence["executables"] = hashes.clone();
        let mut call = Calls {
            c: &mut c,
            tools,
            sequence: 0,
        };
        let mut guest = Guest {
            uid,
            runner,
            fixture,
            data,
            env: guest_env(revision),
            drop: drop_privileges(uid),
        };
        call.c.evidence["guest_identity"] =
            guest.probe(&mut call, "identity", CHUNK, Space::Enospc, None)?;
        guest.maintenance(&mut call, revision, Space::Enospc, &seed.join("seed"))?;
        ensure!(
            inventory(source, &[])? == frozen,
            "frozen engine source changed"
        );
        ensure!(
            executable_hashes(&call.tools)? == hashes
                && sha(&current)? == runner_hash
                && sha(&guest.runner)? == runner_hash
                && sha(&guest.fixture)? == fixture_hash,
            "maintenance tools changed"
        );
        Ok(())
    })();
    c.evidence["artifact_sha256"] =
        json!(inventory(&c.output, &["evidence.json", "cargo-target"])?);
    c.evidence["verified"] = json!(result.is_ok());
    c.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch as Temp;
    #[test]
    fn guest_runner_and_engine_fixture_have_independent_size_limits() {
        let temp = Temp::new(&std::env::temp_dir(), "quota-executable-size").unwrap();
        let runner = temp.path().join("runner");
        fs::File::create(&runner)
            .unwrap()
            .set_len(MAX_FIXTURE_BYTES + 1)
            .unwrap();
        assert!(bounded_executable(&runner, MAX_FIXTURE_BYTES).is_err());
        bounded_executable(&runner, MAX_RUNNER_BYTES).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&runner)
            .unwrap()
            .set_len(MAX_RUNNER_BYTES + 1)
            .unwrap();
        assert!(bounded_executable(&runner, MAX_RUNNER_BYTES).is_err());
    }
    #[test]
    fn unsupported_probe_cannot_qualify_the_engine() {
        let temp = Temp::new(&std::env::temp_dir(), "quota-unsupported").unwrap();
        let run = Run::new(temp.path().to_owned());
        for probe_only in [false, true] {
            let path = temp.path().join(format!("proof-{probe_only}"));
            let campaign = Campaign::detached(&run, &path, "unsupported control").unwrap();
            let result = finish_qualification(
                campaign,
                Err(Unsupported("no quota filesystem".into()).into()),
                true,
                probe_only,
            );
            assert_eq!(result.is_ok(), probe_only);
            let proof = crate::delivery::read_json(&path.join("evidence.json")).unwrap();
            assert_eq!(proof["status"], "unsupported");
            assert_eq!(proof["verified"], false);
        }
        let path = temp.path().join("capability");
        let campaign = Campaign::detached(&run, &path, "capability-only control").unwrap();
        finish_qualification(campaign, Ok(()), true, true).unwrap();
        let proof = crate::delivery::read_json(&path.join("evidence.json")).unwrap();
        assert_eq!(proof["status"], "capability-only");
        assert_eq!(proof["verified"], false);
    }

    #[test]
    fn both_quota_profiles_recheck_the_same_tool_inventory_on_completion() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for maintenance in [false, true] {
            let temp = Temp::new(&std::env::temp_dir(), "quota-tool-inventory").unwrap();
            let before = qualification_files(root, maintenance).unwrap();
            for name in before.keys() {
                let target = temp.path().join(name);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::copy(root.join(name), target).unwrap();
            }
            recheck_qualification_files(temp.path(), maintenance, &before).unwrap();
            for name in [
                "xtask/src/supervise.rs",
                "tools/resource-envelope/src/space.rs",
            ] {
                let target = temp.path().join(name);
                if target.is_file() {
                    let original = fs::read(&target).unwrap();
                    fs::write(&target, b"changed").unwrap();
                    assert!(
                        recheck_qualification_files(temp.path(), maintenance, &before).is_err()
                    );
                    fs::write(target, original).unwrap();
                } else {
                    assert!(!maintenance);
                }
            }
        }
    }
    fn scratch(root: &Path) -> Scratch {
        let directory = root.join(format!("weekend-quota-{}-1", "a".repeat(32)));
        fs::create_dir(&directory).unwrap();
        let mut s = Scratch {
            image: directory.join("image.ext4"),
            mount: directory.join("mount"),
            directory,
            image_identity: None,
        };
        s.prepare(4096).unwrap();
        s
    }
    fn loop_list(s: &Scratch, present: bool) -> String {
        json!({"loopdevices":if present {vec![json!({"name":"/dev/loop7","back-file":s.image})]}else{vec![]}}).to_string()
    }
    fn mounted(s: &Scratch, device: &str) -> String {
        json!({"filesystems":[{"source":device,"target":s.mount,"fstype":"ext4"}]}).to_string()
    }
    #[test]
    fn cleanup_continues_after_unmount_timeout_and_proves_both_absences() {
        let t = Temp::new(&std::env::temp_dir(), "quota-clean-retire").unwrap();
        let s = scratch(t.path());
        let mut attached = true;
        let mut present = true;
        let mut detach = 0;
        let result = cleanup_with(
            &s,
            |op| match op {
                CleanupOp::LoopInventory => Ok(Some(loop_list(&s, attached))),
                CleanupOp::Mount => Ok(present.then(|| mounted(&s, "/dev/loop7"))),
                CleanupOp::Unmount => {
                    present = false;
                    anyhow::bail!("command deadline")
                }
                CleanupOp::Detach(name) => {
                    assert_eq!(name, "/dev/loop7");
                    detach += 1;
                    attached = false;
                    Ok(None)
                }
            },
            || {},
        )
        .unwrap();
        assert_eq!(detach, 1);
        assert_eq!(result["verified"], true);
        assert_eq!(result["errors"].as_array().unwrap().len(), 1);
        assert!(!s.directory.exists());
    }
    #[test]
    fn cleanup_mount_identity_drift_never_unmounts_or_detaches() {
        let t = Temp::new(&std::env::temp_dir(), "quota-clean-drift").unwrap();
        let s = scratch(t.path());
        let result = cleanup_with(
            &s,
            |op| match op {
                CleanupOp::LoopInventory => Ok(Some(loop_list(&s, true))),
                CleanupOp::Mount => Ok(Some(mounted(&s, "/dev/other"))),
                _ => panic!("cleanup must not mutate drifted resources"),
            },
            || {},
        )
        .unwrap();
        assert_eq!(result["verified"], false);
        assert!(s.image.exists());
    }
    #[test]
    fn findmnt_query_error_is_not_absence_and_never_removes_scratch() {
        let t = Temp::new(&std::env::temp_dir(), "quota-clean-query").unwrap();
        let s = scratch(t.path());
        let result = cleanup_with(
            &s,
            |op| match op {
                CleanupOp::LoopInventory => Ok(Some(loop_list(&s, false))),
                CleanupOp::Mount => mount_state(Output {
                    stdout: String::new(),
                    stderr: "findmnt query error".into(),
                    pid: 1,
                    exit: 1,
                }),
                _ => panic!("cleanup must not mutate unknown resources"),
            },
            || {},
        )
        .unwrap();
        assert_eq!(result["verified"], false);
        assert!(s.image.exists());
    }
    #[test]
    fn existing_and_dangling_outputs_reject_before_external_admission() {
        let t = Temp::new(&std::env::temp_dir(), "quota-output").unwrap();
        let existing = t.path().join("existing");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("keep"), b"keep").unwrap();
        let dangling = t.path().join("alias");
        std::os::unix::fs::symlink(t.path().join("missing"), &dangling).unwrap();
        let run = Run::new(t.path().to_owned());
        for output in [&existing, &dangling] {
            let opts = Options {
                fixture: PathBuf::from("no-fixture"),
                source: "not-an-id".into(),
                output: output.to_owned(),
                probe_only: false,
                maintenance: false,
                inode_quota: false,
                inode_enospc: false,
            };
            assert!(
                super::run(&run, opts)
                    .unwrap_err()
                    .to_string()
                    .contains("output already exists")
            );
        }
        assert_eq!(fs::read(existing.join("keep")).unwrap(), b"keep");
        assert!(!t.path().join("missing").exists());
    }
    #[test]
    fn source_mismatch_stops_before_archive_or_root_work() {
        let t = Temp::new(&std::env::temp_dir(), "quota-source").unwrap();
        let mut run = Run::new(t.path().to_owned());
        let git = t.path().join("git");
        fs::write(
            &git,
            format!(
                "#!/bin/sh\ncase \"$5\" in rev-parse) printf '%s\\n' {};; *) exit 97;; esac\n",
                "b".repeat(40)
            ),
        )
        .unwrap();
        fs::set_permissions(git, fs::Permissions::from_mode(0o700)).unwrap();
        run.env.insert("PATH".into(), t.path().as_os_str().into());
        let mut c = Campaign::detached(&run, Path::new("proof"), "source rejection").unwrap();
        assert!(
            source_binding(&mut c, &"a".repeat(40))
                .unwrap_err()
                .to_string()
                .contains("exact commit")
        );
        assert_eq!(c.evidence["commands"].as_array().unwrap().len(), 1);
        assert!(!c.output.join("source.tar").exists());
        assert_eq!(
            c.evidence["commands"][0]["argv"][2],
            format!("safe.directory={}", t.path().display())
        );
    }
    #[test]
    fn real_process_capture_caps_output_and_reports_timeout_cleanup() {
        let t = Temp::new(&std::env::temp_dir(), "quota-process").unwrap();
        let run = Run::new(t.path().to_owned());
        let mut c = Campaign::detached(&run, Path::new("proof"), "quota child controls").unwrap();
        let mut call = Calls {
            c: &mut c,
            tools: BTreeMap::new(),
            sequence: 0,
        };
        let overflow = call
            .command(
                cmd(["/bin/sh", "-c", "exec /usr/bin/yes overflow"]),
                10.,
                &[0],
                None,
                false,
            )
            .err()
            .unwrap();
        assert!(overflow.to_string().contains("output limit"));
        assert!(
            fs::metadata(call.c.output.join("quota-1.stdout"))
                .unwrap()
                .len()
                <= LOG_BYTES
        );
        assert!(
            call.command(
                cmd(["/bin/sh", "-c", "exec sleep 30"]),
                0.1,
                &[0],
                None,
                false
            )
            .is_err()
        );
        for i in [0, 1] {
            assert_eq!(
                call.c.evidence["commands"][i]["supervision"]["cleanup_verified"],
                true
            );
            assert!(
                call.c.evidence["commands"][i]["supervision"]["returncode"]
                    .as_i64()
                    .is_some()
            );
        }
        assert_eq!(
            call.c.evidence["commands"][1]["supervision"]["timed_out"],
            true
        );
    }
}
