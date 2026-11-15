//! Bounded POSIX child ownership. Requests (including environments) stay in memory;
//! reports contain status and ownership metadata, never environment values.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{PipeReader, Read, Write},
    path::PathBuf,
    time::Instant,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Option<BTreeMap<String, String>>,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    pub report: Option<PathBuf>,
    pub timeout_seconds: f64,
    pub maximum_bytes: u64,
    #[serde(default)]
    pub exclusive_limit: bool,
    pub file_limit: Option<u64>,
    pub journal: Option<PathBuf>,
    #[serde(default)]
    pub sample_rss: bool,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub pid: Option<u32>,
    pub returncode: Option<i32>,
    pub elapsed_seconds: f64,
    pub failure: Option<String>,
    pub cleanup_failure: Option<String>,
    pub timed_out: bool,
    pub cancelled: bool,
    #[serde(default)]
    pub stopped: bool,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub rss_samples_kib: Vec<u64>,
    #[serde(default)]
    pub rss_sample_count: u64,
    #[serde(default)]
    pub sampled_peak_rss_kib: u64,
    pub cleanup_verified: bool,
    pub owned_descendants: Vec<Identity>,
}
impl Report {
    pub fn success(&self) -> bool {
        self.returncode == Some(0)
            && self.failure.is_none()
            && self.cleanup_failure.is_none()
            && self.cleanup_verified
    }
    pub(crate) fn stopped_cleanly(&self) -> bool {
        self.stopped
            && self.returncode == Some(-libc::SIGKILL)
            && self.failure.is_none()
            && self.cleanup_failure.is_none()
            && self.cleanup_verified
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub pid: i32,
    pub parent: i32,
    pub group: i32,
    pub start: String,
    pub state: String,
}
fn new_file(path: &PathBuf) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .with_context(|| format!("new output {}", path.display()))
}
#[derive(Clone, Copy, Default)]
enum Sampling {
    #[default]
    Full,
    Peak,
}

#[derive(Default)]
struct Execution<'a> {
    sampling: Sampling,
    stop: Option<&'a std::sync::atomic::AtomicBool>,
    input: Option<PipeReader>,
}

pub fn run(request: &Request) -> Result<Report> {
    run_controlled(request, Execution::default())
}

pub(crate) fn run_peak_rss(request: &Request) -> Result<Report> {
    run_controlled(
        request,
        Execution {
            sampling: Sampling::Peak,
            ..Execution::default()
        },
    )
}

pub(crate) fn run_stoppable(
    request: &Request,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<Report> {
    run_controlled(
        request,
        Execution {
            stop: Some(stop),
            ..Execution::default()
        },
    )
}

pub(crate) fn run_interactive(
    request: &Request,
    stop: &std::sync::atomic::AtomicBool,
    input: PipeReader,
) -> Result<Report> {
    run_controlled(
        request,
        Execution {
            stop: Some(stop),
            input: Some(input),
            ..Execution::default()
        },
    )
}

fn run_controlled(request: &Request, execution: Execution<'_>) -> Result<Report> {
    #[cfg(not(unix))]
    {
        let _ = request;
        let _ = execution;
        anyhow::bail!("bounded POSIX supervision is unavailable on this platform");
    }
    #[cfg(unix)]
    {
        let maximum_deadline = match execution.sampling {
            Sampling::Full => 86_400.,
            Sampling::Peak => 86_520.,
        };
        ensure!(
            !request.argv.is_empty() && !request.argv[0].is_empty(),
            "empty command"
        );
        ensure!(
            request.timeout_seconds.is_finite()
                && request.timeout_seconds > 0.
                && request.timeout_seconds <= maximum_deadline,
            "invalid deadline"
        );
        ensure!(
            request.maximum_bytes > 0 && request.maximum_bytes <= 64 * 1024 * 1024,
            "invalid output limit"
        );
        ensure!(
            request
                .report
                .as_ref()
                .is_none_or(|p| p != &request.stdout && p != &request.stderr),
            "report aliases log"
        );
        // Preflight the whole output set before acquiring any path, then use
        // create_new so final-component races and dangling symlinks fail closed.
        for path in [&request.stdout, &request.stderr]
            .into_iter()
            .chain(request.report.iter())
        {
            ensure!(
                fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
                "output already exists: {}",
                path.display()
            );
            ensure!(
                path.parent().is_some_and(|p| p.is_dir()),
                "output parent missing"
            );
        }
        let mut report_file = request.report.as_ref().map(new_file).transpose()?;
        if let Some(f) = &mut report_file {
            f.write_all(b"{\"cleanup_verified\":false,\"failure\":\"supervision incomplete\"}\n")?;
        }
        let started = Instant::now();
        let mut report = Report::default();
        let result = (|| -> Result<()> {
            let stdout = new_file(&request.stdout)?;
            let stderr = if request.stdout == request.stderr {
                stdout.try_clone()?
            } else {
                new_file(&request.stderr)?
            };
            unix::execute(request, stdout, stderr, &mut report, execution)
        })();
        if let Err(error) = result {
            report.failure.get_or_insert(format!("{error:#}"));
        }
        if report.pid.is_none() {
            report.cleanup_verified = true;
        }
        report.elapsed_seconds = started.elapsed().as_secs_f64();
        if let Some(file) = &mut report_file {
            use std::io::{Seek, SeekFrom};
            file.seek(SeekFrom::Start(0))?;
            file.set_len(0)?;
            serde_json::to_writer_pretty(&mut *file, &report)?;
            file.write_all(b"\n")?;
        }
        Ok(report)
    }
}
pub(crate) struct Cancellation(unix::Signals);
impl Cancellation {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self(unix::Signals::new()?))
    }
    pub(crate) fn requested(&self) -> bool {
        self.0.requested()
    }
}

pub fn cli() -> Result<()> {
    let mut data = Vec::new();
    std::io::stdin()
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    ensure!(
        data.len() <= 8 * 1024 * 1024,
        "oversized supervisor request"
    );
    let request: Request = serde_json::from_slice(&data)?;
    serde_json::to_writer(std::io::stdout(), &run(&request)?)?;
    Ok(())
}
pub fn table_cli() -> Result<()> {
    #[cfg(unix)]
    {
        serde_json::to_writer(std::io::stdout(), &process_table()?)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        anyhow::bail!("POSIX process table unavailable")
    }
}
pub(crate) fn process_table() -> Result<BTreeMap<i32, Identity>> {
    unix::table()
}

pub(crate) use unix::nonblocking;

#[cfg(unix)]
mod unix {
    use super::*;
    use std::{
        collections::BTreeSet,
        io::{self, PipeReader},
        os::{
            fd::AsRawFd,
            unix::process::{CommandExt, ExitStatusExt},
        },
        process::{Child, Command, Stdio},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };
    const PROBE_LIMIT: u64 = 8 * 1024 * 1024;
    pub(crate) fn nonblocking(fd: i32) -> Result<()> {
        // SAFETY: fcntl only changes status flags on an owned, live descriptor.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        ensure!(
            flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0,
            "nonblocking pipe: {}",
            io::Error::last_os_error()
        );
        Ok(())
    }
    struct Stream {
        pipe: PipeReader,
        file: File,
        count: u64,
        eof: bool,
    }
    impl Stream {
        fn drain(&mut self, cap: u64, exclusive: bool) -> Result<()> {
            // Bound each turn so an endless writer cannot starve the deadline.
            for _ in 0..16 {
                let mut bytes = [0; 16384];
                match self.pipe.read(&mut bytes) {
                    Ok(0) => {
                        self.eof = true;
                        break;
                    }
                    Ok(n) => {
                        let allowed = n.min(cap.saturating_sub(self.count) as usize);
                        self.file.write_all(&bytes[..allowed])?;
                        self.count += allowed as u64;
                        ensure!(
                            allowed == n && (!exclusive || self.count < cap),
                            "command output limit"
                        );
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(())
        }
    }
    fn exited(pid: u32) -> Result<bool> {
        // SAFETY: waitid writes to initialized siginfo_t, for our unreaped child.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(unsafe { info.si_pid() } != 0)
    }
    fn signal(pid: i32, sig: i32) -> Result<()> {
        // SAFETY: the caller has checked PID/group ownership. No pointers used.
        if unsafe { libc::kill(pid, sig) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error.into())
        }
    }
    pub(super) struct Signals {
        ids: Vec<signal_hook::SigId>,
        cancelled: Arc<AtomicBool>,
    }
    impl Signals {
        pub(super) fn requested(&self) -> bool {
            self.cancelled.load(Ordering::Relaxed)
        }
        pub(super) fn new() -> Result<Self> {
            let mut value = Self {
                ids: vec![],
                cancelled: Arc::new(AtomicBool::new(false)),
            };
            for sig in [libc::SIGINT, libc::SIGTERM] {
                value
                    .ids
                    .push(signal_hook::flag::register(sig, value.cancelled.clone())?);
            }
            Ok(value)
        }
    }
    impl Drop for Signals {
        fn drop(&mut self) {
            for id in self.ids.drain(..) {
                signal_hook::low_level::unregister(id);
            }
        }
    }
    // A trusted non-forking OS probe. Its two pipes and wall time are bounded;
    // it cannot recursively depend on the ownership inspection it provides.
    fn probe(args: &[&str]) -> Result<String> {
        let mut child = Command::new("/bin/ps")
            .args(args)
            .env("LC_ALL", "C")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut pipe = child.stdout.take().context("probe stdout")?;
        let result = (|| -> Result<String> {
            nonblocking(pipe.as_raw_fd())?;
            let started = Instant::now();
            let mut out = Vec::new();
            let mut eof = false;
            loop {
                let mut bytes = [0; 16384];
                for _ in 0..16 {
                    match pipe.read(&mut bytes) {
                        Ok(0) => {
                            eof = true;
                            break;
                        }
                        Ok(n) => {
                            ensure!(
                                out.len() + n <= PROBE_LIMIT as usize,
                                "process probe output limit"
                            );
                            out.extend_from_slice(&bytes[..n]);
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
                if let Some(status) = child.try_wait()? {
                    ensure!(status.success(), "process probe failed");
                    if eof {
                        return Ok(String::from_utf8(out)?);
                    }
                }
                ensure!(
                    started.elapsed() < Duration::from_secs(2),
                    "process probe deadline"
                );
                thread::sleep(Duration::from_millis(5));
            }
        })();
        if result.is_err() {
            let _ = child.kill();
            let end = Instant::now() + Duration::from_secs(2);
            loop {
                if child.try_wait()?.is_some() {
                    break;
                }
                ensure!(Instant::now() < end, "process probe could not be reaped");
                thread::sleep(Duration::from_millis(5));
            }
        }
        result
    }
    pub(super) fn table() -> Result<BTreeMap<i32, Identity>> {
        let mut table = BTreeMap::new();
        for line in probe(&["-axo", "pid=,ppid=,pgid=,lstart=,stat="])?.lines() {
            let parts: Vec<_> = line.split_whitespace().collect();
            ensure!(parts.len() == 9, "invalid process identity");
            let row = Identity {
                pid: parts[0].parse()?,
                parent: parts[1].parse()?,
                group: parts[2].parse()?,
                start: parts[3..8].join(" "),
                state: parts[8].to_owned(),
            };
            table.insert(row.pid, row);
        }
        ensure!(
            table.contains_key(&(std::process::id() as i32)),
            "process inventory omits observer"
        );
        Ok(table)
    }
    fn journal(request: &Request, owned: &mut BTreeMap<i32, Identity>, strict: bool) -> Result<()> {
        let Some(path) = &request.journal else {
            return Ok(());
        };
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let mut data = Vec::new();
        file.take(PROBE_LIMIT + 1).read_to_end(&mut data)?;
        let mut bad = data.len() > PROBE_LIMIT as usize;
        for line in data.split_inclusive(|b| *b == b'\n') {
            if !strict && !line.ends_with(b"\n") {
                continue;
            }
            match serde_json::from_slice::<Identity>(line) {
                Ok(row)
                    if row.pid > 1
                        && row.group > 0
                        && !row.start.is_empty()
                        && row.pid != std::process::id() as i32 =>
                {
                    owned.insert(row.pid, row);
                }
                _ => bad = true,
            }
        }
        ensure!(!bad, "unverifiable process journal");
        Ok(())
    }
    fn descendants(
        table: &BTreeMap<i32, Identity>,
        unreaped_root: Option<i32>,
        owned: &mut BTreeMap<i32, Identity>,
    ) {
        let mut parents: Vec<_> = unreaped_root
            .into_iter()
            .chain(owned.keys().copied())
            .collect();
        let mut inspected = BTreeSet::new();
        while let Some(parent) = parents.pop() {
            let root = unreaped_root == Some(parent);
            let known = owned.get(&parent);
            let current = table.get(&parent);
            let matches = known
                .zip(current)
                .is_some_and(|(old, now)| old.start == now.start);
            let has_children = root || matches;
            // The last member can outlive the leader of an observed detached
            // group. A different current holder of the number invalidates it.
            let has_group = root
                || known.is_some_and(|old| old.group == parent && (matches || current.is_none()));
            if !(has_children || has_group) || !inspected.insert(parent) {
                continue;
            }
            for row in table.values() {
                if unreaped_root != Some(row.pid)
                    && row.pid != parent
                    && (has_children && row.parent == parent || has_group && row.group == parent)
                {
                    owned.insert(row.pid, row.clone());
                    parents.push(row.pid);
                }
            }
        }
    }
    trait Inspect {
        fn table(&mut self) -> Result<BTreeMap<i32, Identity>>;
        fn exited(&mut self, pid: u32) -> Result<bool>;
        fn kill_group(&mut self, pid: i32) -> Result<()> {
            signal(-pid, libc::SIGKILL)
        }
        fn reaped(&mut self) -> Result<()> {
            Ok(())
        }
        fn rss(&mut self, pid: u32) -> Result<Option<u64>> {
            #[cfg(target_os = "linux")]
            {
                match fs::read_to_string(format!("/proc/{pid}/status")) {
                    Ok(text) => Ok(text
                        .lines()
                        .find(|l| l.starts_with("VmRSS:"))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|n| n.parse().ok())),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e.into()),
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                match probe(&["-o", "rss=", "-p", &pid.to_string()]) {
                    Ok(value) if !value.trim().is_empty() => Ok(Some(value.trim().parse()?)),
                    Ok(_) => Ok(None),
                    Err(_) if exited(pid)? => Ok(None),
                    Err(e) => Err(e),
                }
            }
        }
    }
    struct System;
    impl Inspect for System {
        fn table(&mut self) -> Result<BTreeMap<i32, Identity>> {
            table()
        }
        fn exited(&mut self, pid: u32) -> Result<bool> {
            exited(pid)
        }
    }
    fn retire(
        child: &mut Child,
        request: &Request,
        report: &mut Report,
        owned: &mut BTreeMap<i32, Identity>,
        inspect: &mut impl Inspect,
    ) -> Result<()> {
        let mut errors = vec![];
        let root = child.id() as i32;
        // Errors in inspection must never bypass direct group kill and reaping.
        match inspect.table() {
            Ok(t) => {
                descendants(&t, Some(root), owned);
            }
            Err(e) => errors.push(format!("ownership inspection: {e:#}")),
        }
        if let Err(e) = journal(request, owned, false) {
            errors.push(format!("{e:#}"));
        }
        if request.journal.is_some() {
            let _ = signal(root, libc::SIGTERM);
            let end = Instant::now() + Duration::from_millis(300);
            while Instant::now() < end {
                match inspect.exited(child.id()) {
                    Ok(true) => break,
                    Err(e) => {
                        errors.push(format!("wait: {e:#}"));
                        break;
                    }
                    _ => thread::sleep(Duration::from_millis(10)),
                }
            }
        }
        if let Err(e) = inspect.kill_group(root) {
            // macOS can report EPERM for a group whose only member is a zombie.
            let gone = inspect.exited(child.id()).unwrap_or(false)
                && inspect.table().is_ok_and(|t| {
                    !t.values()
                        .any(|r| r.group == root && !r.state.starts_with('Z'))
                });
            if !gone {
                errors.push(format!("retire process group: {e:#}"));
            }
        }
        // Reap only after waitid observes exit, preserving group identity until
        // retirement. Use the real wait even if a test/probe inspection failed.
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            match exited(child.id()) {
                Ok(true) => {
                    match child.wait() {
                        Ok(status) => {
                            report.returncode =
                                status.code().or_else(|| status.signal().map(|s| -s))
                        }
                        Err(e) => errors.push(format!("wait after observed exit: {e:#}")),
                    }
                    if let Err(e) = inspect.reaped() {
                        errors.push(format!("reap verification: {e:#}"));
                    }
                    break;
                }
                Err(e) => {
                    errors.push(format!("reap: {e:#}"));
                    break;
                }
                _ if Instant::now() >= end => {
                    errors.push("worker reap deadline".into());
                    break;
                }
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        if let Err(e) = journal(request, owned, true) {
            errors.push(format!("{e:#}"));
        }
        // wait released the leader PID. Only previously owned identities and
        // their groups can seed the remaining cleanup.
        owned.remove(&root);
        let end = Instant::now() + Duration::from_secs(4);
        loop {
            let t = match inspect.table() {
                Ok(t) => t,
                Err(e) => {
                    errors.push(format!("cleanup inspection: {e:#}"));
                    break;
                }
            };
            descendants(&t, None, owned);
            let live: Vec<_> = t
                .values()
                .filter(|r| {
                    owned.get(&r.pid).is_some_and(|o| o.start == r.start)
                        && !r.state.starts_with('Z')
                })
                .collect();
            if live.is_empty() {
                break;
            }
            for row in live {
                if let Err(e) = signal(row.pid, libc::SIGKILL) {
                    errors.push(format!("descendant kill: {e:#}"));
                }
            }
            if Instant::now() >= end {
                errors.push("owned descendant remains running".into());
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        report.owned_descendants = owned.values().cloned().collect();
        ensure!(errors.is_empty(), "{}", errors.join("; "));
        report.cleanup_verified = true;
        Ok(())
    }
    pub(super) fn execute(
        request: &Request,
        stdout: File,
        stderr: File,
        report: &mut Report,
        execution: Execution<'_>,
    ) -> Result<()> {
        execute_with(request, stdout, stderr, report, execution, &mut System)
    }
    fn execute_with(
        request: &Request,
        stdout: File,
        stderr: File,
        report: &mut Report,
        execution: Execution<'_>,
        inspect: &mut impl Inspect,
    ) -> Result<()> {
        let Execution {
            sampling,
            stop,
            input,
        } = execution;
        let signals = Signals::new()?;
        let (out_r, out_w) = std::io::pipe()?;
        let merged = request.stdout == request.stderr;
        let (err_r, err_w) = std::io::pipe()?;
        nonblocking(out_r.as_raw_fd())?;
        nonblocking(err_r.as_raw_fd())?;
        let mut command = Command::new(&request.argv[0]);
        command
            .args(&request.argv[1..])
            .stdin(input.map_or_else(Stdio::null, Stdio::from));
        if let Some(cwd) = &request.cwd {
            command.current_dir(cwd);
        }
        if let Some(env) = &request.env {
            command.env_clear().envs(env);
        }
        if merged {
            command.stderr(out_w.try_clone()?);
        } else {
            command.stderr(err_w.try_clone()?);
        }
        command.stdout(out_w);
        drop(err_w);
        let file_limit = request.file_limit;
        // SAFETY: only async-signal-safe syscalls run between fork and exec.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if let Some(limit) = file_limit {
                    let bound = libc::rlimit {
                        rlim_cur: limit as libc::rlim_t,
                        rlim_max: limit as libc::rlim_t,
                    };
                    if libc::setrlimit(libc::RLIMIT_FSIZE, &bound) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        let mut child = command.spawn().context("spawn child")?;
        drop(command);
        report.pid = Some(child.id());
        let mut out = Stream {
            pipe: out_r,
            file: stdout,
            count: 0,
            eof: false,
        };
        let mut err = Stream {
            pipe: err_r,
            file: stderr,
            count: 0,
            eof: false,
        };
        let started = Instant::now();
        let mut next_sample = started;
        let mut next_journal = started;
        let sample_interval = Duration::from_millis(match sampling {
            Sampling::Full => 50,
            Sampling::Peak => 200,
        });
        let mut owned = BTreeMap::new();
        let observed = (|| -> Result<()> {
            let outer_journal = request
                .env
                .as_ref()
                .and_then(|e| e.get("HELPER_PROCESS_JOURNAL").cloned())
                .or_else(|| std::env::var("HELPER_PROCESS_JOURNAL").ok())
                .map(PathBuf::from);
            if let Some(path) = outer_journal.filter(|p| request.journal.as_ref() != Some(p)) {
                let identity = inspect
                    .table()?
                    .remove(&(child.id() as i32))
                    .context("spawned child identity unavailable")?;
                let mut file = OpenOptions::new().append(true).create(true).open(path)?;
                // Parallel observers may share this journal. Append each row
                // together so their JSON tokens cannot interleave.
                let mut row = serde_json::to_vec(&identity)?;
                row.push(b'\n');
                file.write_all(&row)?;
                file.flush()?;
            }
            loop {
                out.drain(request.maximum_bytes, request.exclusive_limit)?;
                err.drain(request.maximum_bytes, request.exclusive_limit)?;
                if signals.cancelled.load(Ordering::Relaxed) {
                    report.cancelled = true;
                    anyhow::bail!("supervisor cancelled");
                }
                if inspect.exited(child.id())? {
                    break;
                }
                if stop.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                    report.stopped = true;
                    break;
                }
                if started.elapsed().as_secs_f64() >= request.timeout_seconds {
                    report.timed_out = true;
                    anyhow::bail!("command deadline");
                }
                if request.journal.is_some() && Instant::now() >= next_journal {
                    descendants(&inspect.table()?, Some(child.id() as i32), &mut owned);
                    journal(request, &mut owned, false)?;
                    next_journal = Instant::now() + Duration::from_millis(50);
                }
                if Instant::now() >= next_sample {
                    if request.sample_rss
                        && let Some(n) = inspect.rss(child.id())?
                    {
                        report.rss_sample_count += 1;
                        report.sampled_peak_rss_kib = report.sampled_peak_rss_kib.max(n);
                        if matches!(sampling, Sampling::Full) {
                            report.rss_samples_kib.push(n);
                        }
                    }
                    next_sample = Instant::now() + sample_interval;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        })();
        if let Err(e) = observed {
            report.failure = Some(format!("{e:#}"));
        }
        if let Err(e) = retire(&mut child, request, report, &mut owned, inspect) {
            report.cleanup_failure = Some(format!("{e:#}"));
        }
        let end = Instant::now() + Duration::from_secs(1);
        while !(out.eof && err.eof) {
            if let Err(e) = out
                .drain(request.maximum_bytes, request.exclusive_limit)
                .and_then(|()| err.drain(request.maximum_bytes, request.exclusive_limit))
            {
                report.failure.get_or_insert(format!("{e:#}"));
                break;
            }
            if Instant::now() >= end {
                report
                    .failure
                    .get_or_insert("output pipe did not close after retirement".into());
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        report.stdout_bytes = out.count;
        report.stderr_bytes = err.count;
        if report.returncode != Some(0) && report.failure.is_none() && !report.stopped_cleanly() {
            report.failure = Some(format!("child exit {:?}", report.returncode));
        }
        Ok(())
    }
    #[cfg(test)]
    mod controls {
        use super::*;
        #[test]
        fn discovery_walks_known_ancestors_and_never_reuses_a_reaped_root_pid() {
            let row = |pid, parent, group, start: &str| Identity {
                pid,
                parent,
                group,
                start: start.to_owned(),
                state: "S".to_owned(),
            };
            let known = row(101, 100, 100, "known");
            let mut owned = BTreeMap::from([(known.pid, known.clone())]);
            let detached = row(102, 101, 102, "detached");
            let same_group_orphan = row(103, 1, 102, "orphan");
            let table = BTreeMap::from([
                (101, known),
                (102, detached),
                (103, same_group_orphan.clone()),
            ]);
            descendants(&table, Some(100), &mut owned);
            assert_eq!(owned.keys().copied().collect::<Vec<_>>(), [101, 102, 103]);

            // The root has been reaped. A tracked detached group still has an
            // orphan, while the old root and another tracked PID were reused.
            owned.remove(&103);
            let table = BTreeMap::from([
                (100, row(100, 1, 100, "unrelated root")),
                (101, row(101, 1, 101, "unrelated worker")),
                (103, same_group_orphan),
                (104, row(104, 100, 100, "unrelated group member")),
                (105, row(105, 101, 105, "unrelated child")),
            ]);
            descendants(&table, None, &mut owned);
            assert_eq!(owned.keys().copied().collect::<Vec<_>>(), [101, 102, 103]);
            assert_eq!(owned[&101].start, "known");
        }

        struct Fault {
            mode: &'static str,
        }
        impl Inspect for Fault {
            fn table(&mut self) -> Result<BTreeMap<i32, Identity>> {
                match self.mode {
                    "exit" | "timeout" | "malformed" => {
                        anyhow::bail!("process probe {}", self.mode)
                    }
                    _ => table(),
                }
            }
            fn exited(&mut self, pid: u32) -> Result<bool> {
                if self.mode == "both" {
                    anyhow::bail!("observation failed")
                }
                exited(pid)
            }
            fn kill_group(&mut self, pid: i32) -> Result<()> {
                signal(-pid, libc::SIGKILL)?;
                if self.mode == "both" {
                    anyhow::bail!("group permission denied")
                }
                Ok(())
            }
            fn reaped(&mut self) -> Result<()> {
                if self.mode == "wait" {
                    anyhow::bail!("wait failed after reap")
                }
                Ok(())
            }
            fn rss(&mut self, _: u32) -> Result<Option<u64>> {
                if self.mode == "sampler-exit" {
                    anyhow::bail!("RSS probe failed");
                }
                anyhow::bail!("sampler timeout")
            }
        }
        #[test]
        fn exited_leader_is_unreaped_when_its_group_is_retired() {
            struct Reserved(bool);
            impl Inspect for Reserved {
                fn table(&mut self) -> Result<BTreeMap<i32, Identity>> {
                    table()
                }
                fn exited(&mut self, pid: u32) -> Result<bool> {
                    exited(pid)
                }
                fn kill_group(&mut self, pid: i32) -> Result<()> {
                    assert!(exited(pid as u32)?);
                    self.0 = true;
                    signal(-pid, libc::SIGKILL)
                }
            }
            let (_owned, request) = super::super::tests::fixture("exit 2");
            let mut report = Report::default();
            let mut inspect = Reserved(false);
            execute_with(
                &request,
                new_file(&request.stdout).unwrap(),
                new_file(&request.stderr).unwrap(),
                &mut report,
                Execution::default(),
                &mut inspect,
            )
            .unwrap();
            assert!(inspect.0);
            assert_eq!(report.returncode, Some(2));
            assert!(report.cleanup_verified);
        }
        #[test]
        fn probe_wait_sampler_and_combined_failures_still_reap() {
            for mode in [
                "exit",
                "timeout",
                "malformed",
                "wait",
                "sampler",
                "sampler-summary",
                "sampler-exit",
                "both",
            ] {
                let (_owned, mut request) = super::super::tests::fixture("exec sleep 30");
                request.timeout_seconds = 0.1;
                request.sample_rss = mode.starts_with("sampler");
                let mut report = Report::default();
                execute_with(
                    &request,
                    new_file(&request.stdout).unwrap(),
                    new_file(&request.stderr).unwrap(),
                    &mut report,
                    Execution {
                        sampling: if matches!(mode, "sampler-summary" | "sampler-exit") {
                            Sampling::Peak
                        } else {
                            Sampling::Full
                        },
                        ..Execution::default()
                    },
                    &mut Fault { mode },
                )
                .unwrap();
                assert_eq!(report.returncode, Some(-9), "{mode}: {report:?}");
                assert!(!report.success());
                if request.sample_rss {
                    let diagnostic = if mode == "sampler-exit" {
                        "RSS probe failed"
                    } else {
                        "sampler timeout"
                    };
                    assert!(report.failure.as_ref().unwrap().contains(diagnostic));
                    assert!(report.cleanup_verified);
                } else {
                    assert!(report.cleanup_failure.is_some(), "{mode}: {report:?}");
                }
                if mode == "both" {
                    assert!(report.failure.unwrap().contains("observation failed"));
                }
                // The real leader has been reaped even when a probe or wait
                // verification fails. This is independent of report contents.
                let rc = unsafe {
                    libc::waitpid(
                        report.pid.unwrap() as i32,
                        std::ptr::null_mut(),
                        libc::WNOHANG,
                    )
                };
                assert_eq!(rc, -1);
                assert_eq!(
                    io::Error::last_os_error().raw_os_error(),
                    Some(libc::ECHILD)
                );
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::support::Scratch;
    pub(super) fn fixture(body: &str) -> (Scratch, Request) {
        let scratch = Scratch::new(&std::env::temp_dir(), "xtask-supervision").unwrap();
        let request = Request {
            argv: vec!["/bin/sh".into(), "-c".into(), body.into()],
            cwd: None,
            env: None,
            stdout: scratch.path().join("stdout"),
            stderr: scratch.path().join("stderr"),
            report: Some(scratch.path().join("report.json")),
            timeout_seconds: 2.,
            maximum_bytes: 1024,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        };
        (scratch, request)
    }
    #[test]
    fn bounds_both_streams_and_exact_limit() {
        for stream in [1, 2] {
            for (n, exclusive, success) in [
                (1024, false, true),
                (1024, true, false),
                (1025, false, false),
                (1025, true, false),
            ] {
                let (_owned, mut request) = fixture(&format!("head -c {n} /dev/zero >&{stream}"));
                request.exclusive_limit = exclusive;
                let report = run(&request).unwrap();
                assert_eq!(report.success(), success, "{report:?}");
                assert!(report.cleanup_verified);
                assert!(fs::metadata(&request.stdout).unwrap().len() <= 1024);
                assert!(fs::metadata(&request.stderr).unwrap().len() <= 1024);
            }
        }
    }
    #[test]
    fn spawn_error_persists_terminal_report() {
        let (_owned, mut request) = fixture("");
        request.argv = vec!["/definitely/missing/xtask-child".into()];
        let report = run(&request).unwrap();
        assert!(report.failure.as_ref().unwrap().contains("spawn child"));
        assert!(report.pid.is_none());
        assert!(report.cleanup_failure.is_none());
        let saved: Report =
            serde_json::from_slice(&fs::read(request.report.unwrap()).unwrap()).unwrap();
        assert!(saved.failure.is_some());
        assert!(saved.returncode.is_none());
    }
    #[test]
    fn timeout_reaps_leader_and_cleans_descendants() {
        let (_owned, mut request) = fixture("sleep 30 & wait");
        request.timeout_seconds = 0.25;
        let report = run(&request).unwrap();
        assert!(report.timed_out);
        assert_eq!(report.returncode, Some(-9));
        assert!(report.cleanup_verified, "{report:?}");
        assert!(!report.owned_descendants.is_empty());
        assert!(report.elapsed_seconds < 8.);
    }
    #[test]
    fn exited_leader_does_not_wait_for_descendant_pipe_eof() {
        let (_owned, request) = fixture("sleep 30 & exit 2");
        let report = run(&request).unwrap();
        assert_eq!(report.returncode, Some(2));
        assert!(report.cleanup_verified, "{report:?}");
        assert!(!report.timed_out);
        assert!(report.elapsed_seconds < 2.);
    }
    #[test]
    fn all_paths_admitted_before_launch_and_existing_data_untouched() {
        let (_owned, request) = fixture("exit 99");
        fs::write(&request.stderr, b"preserve").unwrap();
        assert!(run(&request).is_err());
        assert!(!request.stdout.exists());
        assert!(!request.report.unwrap().exists());
        assert_eq!(fs::read(&request.stderr).unwrap(), b"preserve");
    }
    #[test]
    fn file_size_limit_is_inherited_by_descendants() {
        let (owned, mut request) = fixture("");
        let file = owned.path().join("oversized");
        request.argv[2] = "dd if=/dev/zero of=\"$1\" bs=4096 count=1".into();
        request
            .argv
            .extend(["fixture".into(), file.to_string_lossy().into_owned()]);
        request.file_limit = Some(1024);
        let report = run(&request).unwrap();
        assert!(!report.success());
        assert!(report.cleanup_verified);
        assert_eq!(fs::metadata(file).unwrap().len(), 1024);
    }
    #[test]
    fn malformed_journal_fails_after_direct_worker_cleanup() {
        let (owned, mut request) = fixture("exec sleep 30");
        let journal = owned.path().join("journal");
        fs::write(&journal, b"{broken\n").unwrap();
        request.journal = Some(journal);
        let report = run(&request).unwrap();
        assert!(report.failure.as_ref().unwrap().contains("journal"));
        assert!(report.cleanup_failure.as_ref().unwrap().contains("journal"));
        assert!(report.returncode.is_some());
        assert!(!report.cleanup_verified);
    }

    #[test]
    fn detached_sessions_are_retired_after_timeout_exit_or_malformed_journal() {
        const MODE: &str = "WEEKEND_TEST_DETACHED_SESSION";
        const NAME: &str = "supervise::tests::detached_sessions_are_retired_after_timeout_exit_or_malformed_journal";
        if let Ok(mode) = std::env::var(MODE) {
            detached_session_worker(&mode);
            return;
        }
        for mode in ["timeout", "early", "orphan", "malformed"] {
            let (owned, mut request) = fixture("");
            let journal = owned.path().join("journal");
            request.argv = vec![
                std::env::current_exe().unwrap().to_str().unwrap().into(),
                "--exact".into(),
                NAME.into(),
            ];
            request.cwd = Some(owned.path().to_owned());
            let mut env: BTreeMap<_, _> = std::env::vars().collect();
            env.insert(MODE.into(), mode.into());
            request.env = Some(env);
            request.journal = Some(journal);
            request.timeout_seconds = 3.;
            let report = run(&request).unwrap();
            let pid: i32 = fs::read_to_string(owned.path().join("detached.pid"))
                .unwrap_or_else(|err| panic!("{mode}: {err}; {report:?}"))
                .trim()
                .parse()
                .unwrap();
            assert!(
                process_table()
                    .unwrap()
                    .get(&pid)
                    .is_none_or(|row| row.state.starts_with('Z')),
                "{mode}: {report:?}"
            );
            assert!(report.owned_descendants.iter().any(|row| row.pid == pid));
            assert_eq!(report.cleanup_verified, mode != "malformed", "{report:?}");
            match mode {
                "timeout" => assert!(report.timed_out, "{report:?}"),
                "early" => assert_eq!(report.returncode, Some(2), "{report:?}"),
                "orphan" => assert!(report.success(), "{report:?}"),
                "malformed" => {
                    assert!(report.cleanup_failure.unwrap().contains("journal"));
                }
                _ => unreachable!(),
            }
        }
    }

    fn detached_session_worker(mode: &str) {
        use std::{
            os::unix::process::CommandExt,
            process::{Command, Stdio},
            thread,
            time::Duration,
        };
        let mut command = if mode == "orphan" {
            let mut command = Command::new("/bin/sh");
            command.args([
                "-c",
                r#"sleep 30 & printf '%s\n' "$!" > detached.pid; read -r release"#,
            ]);
            command.stdin(Stdio::piped());
            command
        } else {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        };
        // SAFETY: setsid is async-signal-safe and only affects the forked child.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        let pid = child.id() as i32;
        let row = process_table().unwrap().remove(&pid).unwrap();
        assert_eq!(row.group, pid);
        let mut line = serde_json::to_vec(&row).unwrap();
        line.push(b'\n');
        if mode == "malformed" {
            line.extend(b"{broken\n");
        }
        if mode != "orphan" {
            fs::write("detached.pid", pid.to_string()).unwrap();
        }
        fs::write("journal", &line).unwrap();
        match mode {
            "early" => std::process::exit(2),
            "orphan" => {
                writeln!(child.stdin.take().unwrap(), "release").unwrap();
                assert!(child.wait().unwrap().success());
            }
            _ => {
                thread::sleep(Duration::from_secs(30));
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
