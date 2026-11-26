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
#[cfg_attr(test, derive(Clone))]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_rss_samples: Vec<GroupRssSample>,
    #[serde(default)]
    pub rss_sample_count: u64,
    #[serde(default)]
    pub sampled_peak_rss_kib: u64,
    pub cleanup_verified: bool,
    pub owned_descendants: Vec<Identity>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRssSample {
    pub leader_kib: u64,
    pub group_kib: u64,
    pub members: u32,
}
impl Report {
    pub fn success(&self) -> bool {
        self.returncode == Some(0)
            && self.failure.is_none()
            && self.cleanup_failure.is_none()
            && self.cleanup_verified
    }
    pub(crate) fn exited_with_error(&self, code: i32) -> bool {
        code > 0
            && self.returncode == Some(code)
            && self.cleanup_verified
            && self.cleanup_failure.is_none()
            && !self.timed_out
            && !self.cancelled
            && !self.stopped
            && self.failure.as_deref() == Some(&format!("child exit Some({code})"))
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
    #[cfg(unix)]
    Group,
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

#[cfg(unix)]
pub(crate) fn run_group_rss(request: &Request) -> Result<Report> {
    run_controlled(
        request,
        Execution {
            sampling: Sampling::Group,
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
            Sampling::Full | Sampling::Group => 86_400.,
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
    #[cfg(not(target_os = "linux"))]
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
    #[cfg(target_os = "linux")]
    fn proc_identity(bytes: &[u8], pid: i32) -> Result<Option<Identity>> {
        // comm is parenthesized but can itself contain spaces, newlines and
        // parentheses. Only the numeric prefix and fields after comm are UTF-8.
        let begin = bytes
            .iter()
            .position(|&b| b == b'(')
            .context("missing comm")?;
        let end = bytes
            .iter()
            .rposition(|&b| b == b')')
            .context("missing comm end")?;
        ensure!(
            begin < end && bytes.get(end + 1) == Some(&b' '),
            "invalid process name delimiters"
        );
        let reported: i32 = std::str::from_utf8(&bytes[..begin])?.trim_end().parse()?;
        ensure!(reported == pid && pid > 0, "process ID changed");
        let mut fields = std::str::from_utf8(&bytes[end + 2..])?.split_ascii_whitespace();
        let state = fields.next().context("missing process state")?;
        ensure!(
            state.len() == 1 && state.as_bytes()[0].is_ascii_alphabetic(),
            "invalid process state"
        );
        let parent: i32 = fields.next().context("missing parent PID")?.parse()?;
        let group: i32 = fields.next().context("missing process group")?.parse()?;
        // Fields 6–21 precede starttime at position 22 in proc_pid_stat(5).
        let start: u64 = fields
            .nth(16)
            .context("missing process starttime")?
            .parse()?;
        // do_task_stat can keep its initial PPID and PGID after reaping clears
        // the signal handler. The state was sampled before that check.
        if parent == 0 && group == -1 {
            return Ok(None);
        }
        ensure!(parent >= 0 && group >= 0, "negative process identity");
        Ok(Some(Identity {
            pid,
            parent,
            group,
            start: start.to_string(),
            state: state.to_owned(),
        }))
    }

    #[cfg(target_os = "linux")]
    fn proc_stats(mut observe: impl FnMut(i32, &[u8]) -> Result<()>) -> Result<()> {
        const STAT_LIMIT: usize = 4096;
        let started = Instant::now();
        let mut bytes = Vec::new();
        let mut total = 0;
        for entry in fs::read_dir("/proc")? {
            ensure!(
                started.elapsed() < Duration::from_secs(2),
                "process inventory deadline"
            );
            let entry = entry?;
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|name| name.parse::<i32>().ok()) else {
                continue;
            };
            bytes.clear();
            let stat = entry.path().join("stat");
            let read = File::open(&stat)
                .and_then(|file| file.take((STAT_LIMIT + 1) as u64).read_to_end(&mut bytes));
            match read {
                Ok(0) => continue,
                Err(error)
                    if error.kind() == io::ErrorKind::NotFound
                        || error.raw_os_error() == Some(libc::ESRCH) =>
                {
                    continue;
                }
                other => {
                    other.with_context(|| format!("read process {pid} stat"))?;
                }
            }
            ensure!(bytes.len() <= STAT_LIMIT, "process stat length limit");
            total += bytes.len() as u64;
            ensure!(total <= PROBE_LIMIT, "process inventory byte limit");
            observe(pid, &bytes).with_context(|| format!("process {pid} stat"))?;
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub(super) fn table() -> Result<BTreeMap<i32, Identity>> {
        let mut table = BTreeMap::new();
        proc_stats(|pid, bytes| {
            if let Some(row) = proc_identity(bytes, pid)? {
                table.insert(pid, row);
            }
            Ok(())
        })?;
        ensure!(
            table.contains_key(&(std::process::id() as i32)),
            "process inventory omits observer"
        );
        Ok(table)
    }

    #[cfg(not(target_os = "linux"))]
    pub(super) fn table() -> Result<BTreeMap<i32, Identity>> {
        let mut table = BTreeMap::new();
        for line in probe(&["-axo", "pid=,ppid=,pgid=,lstart=,stat="])?.lines() {
            let parts: Vec<_> = line.split_whitespace().collect();
            ensure!(parts.len() == 9, "invalid process identity");
            let row = Identity {
                pid: parts[0].parse().context("invalid process ID")?,
                parent: parts[1].parse().context("invalid parent process ID")?,
                group: parts[2].parse().context("invalid process group ID")?,
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

    fn add_group_member(sample: &mut GroupRssSample, leader: bool, kib: u64) -> Result<()> {
        sample.members = sample
            .members
            .checked_add(1)
            .context("RSS group count overflow")?;
        sample.group_kib = sample
            .group_kib
            .checked_add(kib)
            .context("RSS group total overflow")?;
        if leader {
            sample.leader_kib = kib;
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn group_rss(group: i32) -> Result<Option<GroupRssSample>> {
        // SAFETY: sysconf takes a constant with no pointers or side effects.
        let page_bytes = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) })?;
        ensure!(page_bytes > 0, "invalid process resident page size");
        let mut sample = GroupRssSample::default();
        proc_stats(|pid, bytes| {
            if let Some(row) = proc_identity(bytes, pid)?
                && row.group == group
                && !matches!(row.state.as_str(), "Z" | "X" | "x")
            {
                // Fields 3 through 24 follow comm. RSS is in resident pages.
                let end = bytes
                    .iter()
                    .rposition(|&b| b == b')')
                    .context("missing comm end")?;
                let pages: u64 = std::str::from_utf8(&bytes[end + 2..])?
                    .split_ascii_whitespace()
                    .nth(21)
                    .context("missing resident pages")?
                    .parse()?;
                let kib = pages
                    .checked_mul(page_bytes)
                    .context("RSS page conversion overflow")?
                    / 1024;
                add_group_member(&mut sample, pid == group, kib)?;
            }
            Ok(())
        })?;
        Ok((sample.leader_kib > 0).then_some(sample))
    }

    #[cfg(not(target_os = "linux"))]
    fn group_rss(group: i32) -> Result<Option<GroupRssSample>> {
        let mut sample = GroupRssSample::default();
        for line in probe(&["-axo", "pid=,pgid=,rss=,stat="])?.lines() {
            let mut parts = line.split_ascii_whitespace();
            let pid: i32 = parts.next().context("missing RSS PID")?.parse()?;
            let pgid: i32 = parts.next().context("missing RSS process group")?.parse()?;
            let kib: u64 = parts.next().context("missing resident size")?.parse()?;
            let state = parts.next().context("missing RSS process state")?;
            ensure!(parts.next().is_none(), "invalid process RSS record");
            if pgid == group && !state.starts_with('Z') {
                add_group_member(&mut sample, pid == group, kib)?;
            }
        }
        Ok((sample.leader_kib > 0).then_some(sample))
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
        let mut pending_group_error = None;
        if let Err(e) = inspect.kill_group(root) {
            // Only macOS's native EPERM is eligible for the zombie-only
            // exception. An immediate observation can precede exit.
            if cfg!(target_os = "macos")
                && e.downcast_ref::<io::Error>()
                    .is_some_and(|error| error.raw_os_error() == Some(libc::EPERM))
            {
                pending_group_error = Some(e);
            } else {
                errors.push(format!("retire process group: {e:#}"));
            }
        }
        // Reap only after waitid observes exit, preserving group identity until
        // retirement. Use the real wait even if a test/probe inspection failed.
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            match exited(child.id()) {
                Ok(true) => {
                    if let Some(error) = pending_group_error.take() {
                        // waitid(WNOWAIT) still pins the root PID. Prove group
                        // absence before wait releases it, never afterward.
                        match inspect.table() {
                            Ok(t)
                                if !t.values().any(|row| {
                                    row.group == root && !row.state.starts_with('Z')
                                }) => {}
                            Ok(_) => errors.push(format!("retire process group: {error:#}")),
                            Err(probe) => errors.push(format!(
                                "retire process group: {error:#}; retirement inspection: {probe:#}"
                            )),
                        }
                    }
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
        if let Some(error) = pending_group_error {
            errors.push(format!("retire process group: {error:#}"));
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
            Sampling::Full | Sampling::Group => 50,
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
                    let resident = if !request.sample_rss {
                        None
                    } else if matches!(sampling, Sampling::Group) {
                        group_rss(child.id() as i32)?.map(|sample| {
                            report.group_rss_samples.push(sample);
                            sample.leader_kib
                        })
                    } else {
                        inspect.rss(child.id())?
                    };
                    if let Some(n) = resident {
                        report.rss_sample_count += 1;
                        report.sampled_peak_rss_kib = report.sampled_peak_rss_kib.max(n);
                        if !matches!(sampling, Sampling::Peak) {
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

        #[cfg(target_os = "linux")]
        #[test]
        fn proc_identity_reports_the_first_malformed_field() {
            let rejected: &[(&str, &[u8], &str)] = &[
                ("opening delimiter", b"42 name) R", "missing comm"),
                ("closing delimiter", b"42 (name R", "missing comm end"),
                (
                    "delimiter order",
                    b"42 ) R (",
                    "invalid process name delimiters",
                ),
                (
                    "delimiter space",
                    b"42 (name)R",
                    "invalid process name delimiters",
                ),
                ("PID mismatch", b"43 (name) R", "process ID changed"),
                ("absent state", b"42 (name) ", "missing process state"),
                ("state width", b"42 (name) RR", "invalid process state"),
                ("state alphabet", b"42 (name) 8", "invalid process state"),
                ("absent parent", b"42 (name) R", "missing parent PID"),
                ("absent group", b"42 (name) R 1", "missing process group"),
                (
                    "absent start",
                    b"42 (name) R 1 42",
                    "missing process starttime",
                ),
            ];
            for &(label, bytes, expected) in rejected {
                let error = proc_identity(bytes, 42).expect_err(label);
                assert_eq!(error.to_string(), expected, "{label}");
            }
            assert_eq!(
                proc_identity(b"0 (name) R", 0).unwrap_err().to_string(),
                "process ID changed",
                "nonpositive PID"
            );

            for (label, bytes) in [
                ("UTF-8 prefix", &b"\xff (name) R"[..]),
                ("UTF-8 suffix", &b"42 (name) \xff"[..]),
            ] {
                let error = proc_identity(bytes, 42).expect_err(label);
                assert!(error.is::<std::str::Utf8Error>(), "{label}: {error:#}");
            }

            let bad_start = format!("42 (name) R 1 42 {}-1", "0 ".repeat(16));
            for (label, bytes) in [
                ("PID parse", &b"x (name) R"[..]),
                ("parent parse", &b"42 (name) R x 42"[..]),
                ("group parse", &b"42 (name) R 1 x"[..]),
                ("start parse", bad_start.as_bytes()),
            ] {
                let error = proc_identity(bytes, 42).expect_err(label);
                assert!(error.is::<std::num::ParseIntError>(), "{label}: {error:#}");
            }
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn proc_identity_distinguishes_reaped_and_live_rows() {
            let stat = |parent, group| {
                let mut bytes = b"42 (main (space) \xff\ncomm)) R ".to_vec();
                bytes.extend_from_slice(
                    format!("{parent} {group} {}{} 123 456\n", "0 ".repeat(16), u64::MAX)
                        .as_bytes(),
                );
                bytes
            };
            let row = proc_identity(&stat(1, 42), 42).unwrap().unwrap();
            assert_eq!((row.pid, row.parent, row.group), (42, 1, 42));
            assert_eq!(row.start, u64::MAX.to_string());
            assert_eq!(row.state, "R");
            assert!(proc_identity(&stat(0, -1), 42).unwrap().is_none());
            let row = proc_identity(&stat(0, 0), 42).unwrap().unwrap();
            assert_eq!((row.parent, row.group), (0, 0));
            for (parent, group) in [(1, -1), (-1, 42)] {
                let error = proc_identity(&stat(parent, group), 42).unwrap_err();
                assert_eq!(
                    error.to_string(),
                    "negative process identity",
                    "{parent}, {group}"
                );
            }
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn proc_identity_matches_the_running_observer() {
            let pid = std::process::id() as i32;
            let raw = fs::read("/proc/self/stat").unwrap();
            let direct = proc_identity(&raw, pid).unwrap().unwrap();
            let inventory = table().unwrap();
            let observed = &inventory[&pid];
            assert_eq!(
                (observed.pid, observed.parent, observed.group),
                (pid, direct.parent, direct.group)
            );
            assert_eq!(observed.start, direct.start);
        }

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

        #[cfg(target_os = "linux")]
        #[test]
        fn proc_stat_reads_exact_start_ticks_and_ignores_the_process_name() {
            let mut bytes = b"42 (name (with) spaces\xff\n) Z 7 42".to_vec();
            for _ in 0..16 {
                bytes.extend_from_slice(b" 0");
            }
            bytes.extend_from_slice(b" 4294967598 33\n");
            let row = proc_identity(&bytes, 42).unwrap().unwrap();
            assert_eq!((row.pid, row.parent, row.group), (42, 7, 42));
            assert_eq!(row.state, "Z");
            assert_eq!(row.start, "4294967598");
            assert!(proc_identity(&bytes, 41).is_err());
            assert!(proc_identity(b"42 (short) Z 7 42", 42).is_err());
            assert!(proc_identity(b"42 (no end Z 7 42", 42).is_err());
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn proc_stat_skips_a_row_reaped_after_the_state_sample() {
            let fields = " 0".repeat(16);
            let reaped = format!("42 (short lived) R 0 -1{fields} 12003\n");
            assert!(proc_identity(reaped.as_bytes(), 42).unwrap().is_none());
            let malformed = format!("42 (short lived) R 1 -1{fields} 12003\n");
            assert!(proc_identity(malformed.as_bytes(), 42).is_err());
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
                if self.mode == "sampler-summary" {
                    // Model a runner that delays the first observation.
                    thread::sleep(Duration::from_millis(125));
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

        struct OwnedRoot(Child);
        impl Drop for OwnedRoot {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        struct GroupError {
            root: Identity,
            mode: &'static str,
            killed: bool,
            reaped: bool,
            immediate_checks: usize,
            proof_tables: usize,
        }
        impl Inspect for GroupError {
            fn table(&mut self) -> Result<BTreeMap<i32, Identity>> {
                if self.reaped {
                    return Ok(BTreeMap::new());
                }
                if !self.killed {
                    return Ok(BTreeMap::from([(self.root.pid, self.root.clone())]));
                }
                self.proof_tables += 1;
                if self.mode == "late" {
                    return Ok(BTreeMap::from([(self.root.pid, self.root.clone())]));
                }
                // A positive native waitid still succeeds here only while the
                // real owned child remains unreaped and its PID is reserved.
                assert!(exited(self.root.pid as u32)?);
                if self.mode == "probe" {
                    anyhow::bail!("injected retirement table failure");
                }
                if self.mode == "live" {
                    // Never invent a foreign PID that cleanup might signal.
                    let mut live = self.root.clone();
                    live.state = "S".into();
                    return Ok(BTreeMap::from([(live.pid, live)]));
                }
                let rows = table()?;
                assert!(
                    !rows
                        .values()
                        .any(|row| { row.group == self.root.pid && !row.state.starts_with('Z') })
                );
                Ok(rows)
            }
            fn exited(&mut self, pid: u32) -> Result<bool> {
                assert_eq!(pid, self.root.pid as u32);
                self.immediate_checks += 1;
                if matches!(self.mode, "delayed" | "late") {
                    Ok(false)
                } else {
                    exited(pid)
                }
            }
            fn kill_group(&mut self, pid: i32) -> Result<()> {
                assert_eq!(pid, self.root.pid);
                self.killed = true;
                if self.mode != "late" {
                    // Signal only our actual unreaped child. The seam injects
                    // the group syscall result, not ownership or native waitid.
                    signal(pid, libc::SIGKILL)?;
                    let end = Instant::now() + Duration::from_secs(2);
                    while !exited(pid as u32)? {
                        ensure!(Instant::now() < end, "owned root did not exit");
                        thread::sleep(Duration::from_millis(1));
                    }
                }
                match self.mode {
                    "other-io" => Err(io::Error::from_raw_os_error(libc::EIO).into()),
                    "access" => Err(io::Error::from_raw_os_error(libc::EACCES).into()),
                    "kind-only" => {
                        Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected").into())
                    }
                    "text" => anyhow::bail!("injected group permission denied"),
                    _ => Err(io::Error::from_raw_os_error(libc::EPERM).into()),
                }
            }
            fn reaped(&mut self) -> Result<()> {
                self.reaped = true;
                Ok(())
            }
        }

        fn group_error_control(mode: &'static str) -> (Result<()>, Report, GroupError) {
            let (_owned, request) = super::super::tests::fixture("");
            let mut command = Command::new("/bin/sh");
            command
                .args(["-c", "read -r release"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            // SAFETY: only setsid runs in the child before exec.
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut child = OwnedRoot(command.spawn().unwrap());
            let pid = child.0.id() as i32;
            let identity = table().unwrap().remove(&pid).unwrap();
            assert_eq!(identity.group, pid);
            let mut inspect = GroupError {
                root: identity,
                mode,
                killed: false,
                reaped: false,
                immediate_checks: 0,
                proof_tables: 0,
            };
            let mut report = Report {
                pid: Some(child.0.id()),
                ..Report::default()
            };
            let result = retire(
                &mut child.0,
                &request,
                &mut report,
                &mut BTreeMap::new(),
                &mut inspect,
            );
            // The late-exit negative intentionally leaves the child alive at
            // the bounded reap deadline. Its guard still owns and retires it.
            let _ = child.0.kill();
            child.0.wait().unwrap();
            let rc = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
            assert_eq!(rc, -1);
            assert_eq!(
                io::Error::last_os_error().raw_os_error(),
                Some(libc::ECHILD)
            );
            (result, report, inspect)
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn group_eperm_waits_for_native_exit_and_live_group_proof_before_reap() {
            let (result, report, inspect) = group_error_control("delayed");
            assert!(
                result.is_ok(),
                "{result:?}; {report:?}; immediate={} proof_tables={}",
                inspect.immediate_checks,
                inspect.proof_tables
            );
            assert_eq!(report.returncode, Some(-9));
            assert!(report.cleanup_verified && inspect.reaped);
            assert_eq!(inspect.proof_tables, 1);
        }

        #[test]
        fn arbitrary_group_errors_are_not_hidden_by_an_exited_empty_group() {
            for mode in [
                "other-io",
                "access",
                "kind-only",
                "text",
                #[cfg(not(target_os = "macos"))]
                "native-permission",
            ] {
                let (result, report, inspect) = group_error_control(mode);
                assert!(
                    result.is_err(),
                    "{mode}: {report:?}; immediate={} proof_tables={}",
                    inspect.immediate_checks,
                    inspect.proof_tables
                );
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("retire process group")
                );
                assert_eq!(report.returncode, Some(-9));
                assert!(!report.cleanup_verified);
                assert!(inspect.reaped);
            }
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn group_eperm_cannot_hide_probe_live_group_or_late_exit_failures() {
            for mode in ["probe", "live", "late"] {
                let (result, report, inspect) = group_error_control(mode);
                assert!(result.is_err(), "{mode}: {report:?}");
                let reason = result.unwrap_err().to_string();
                assert!(reason.contains("retire process group"), "{mode}: {reason}");
                assert!(!report.cleanup_verified);
                if mode == "late" {
                    assert!(reason.contains("worker reap deadline"));
                    assert_eq!(report.returncode, None);
                    assert!(!inspect.reaped);
                } else {
                    assert_eq!(report.returncode, Some(-9));
                    assert!(inspect.reaped && inspect.proof_tables > 0);
                }
            }
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
                request.sample_rss = mode.starts_with("sampler");
                // Give the injected sampler error time to fire after observation.
                request.timeout_seconds = if request.sample_rss { 5.0 } else { 0.1 };
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
                    assert!(
                        report.failure.as_ref().unwrap().contains(diagnostic),
                        "{mode}: {report:?}"
                    );
                    assert!(!report.timed_out, "{mode}: {report:?}");
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

    type ReportChange = (&'static str, fn(&mut Report));

    #[test]
    fn success_requires_zero_exit_and_verified_cleanup() {
        let accepted = || Report {
            returncode: Some(0),
            cleanup_verified: true,
            ..Report::default()
        };
        assert!(accepted().success());
        let changes: &[ReportChange] = &[
            ("zero exit", |r| r.returncode = Some(1)),
            ("worker failure", |r| r.failure = Some("fixture".into())),
            ("cleanup failure", |r| {
                r.cleanup_failure = Some("fixture".into())
            }),
            ("cleanup proof", |r| r.cleanup_verified = false),
        ];
        for (condition, change) in changes {
            let mut report = accepted();
            change(&mut report);
            assert!(!report.success(), "ordinary success ignored {condition}");
        }
    }

    #[test]
    fn expected_error_requires_positive_code_exact_reason_and_cleanup() {
        let accepted = |code| Report {
            returncode: Some(code),
            failure: Some(format!("child exit Some({code})")),
            cleanup_verified: true,
            ..Report::default()
        };
        assert!(accepted(42).exited_with_error(42));
        for code in [0, -libc::SIGKILL] {
            assert!(
                !accepted(code).exited_with_error(code),
                "expected error ignored positive code"
            );
        }
        let changes: &[ReportChange] = &[
            ("exit code", |r| r.returncode = Some(43)),
            ("cleanup proof", |r| r.cleanup_verified = false),
            ("cleanup failure", |r| {
                r.cleanup_failure = Some("fixture".into())
            }),
            ("timeout", |r| r.timed_out = true),
            ("cancellation", |r| r.cancelled = true),
            ("explicit stop", |r| r.stopped = true),
            ("exact reason", |r| {
                r.failure = Some("child exit Some(43)".into())
            }),
        ];
        for (condition, change) in changes {
            let mut report = accepted(42);
            change(&mut report);
            assert!(
                !report.exited_with_error(42),
                "expected error ignored {condition}"
            );
        }
    }

    #[test]
    fn clean_stop_requires_requested_stop_sigkill_and_verified_cleanup() {
        let accepted = || Report {
            stopped: true,
            returncode: Some(-libc::SIGKILL),
            cleanup_verified: true,
            ..Report::default()
        };
        assert!(accepted().stopped_cleanly());
        let changes: &[ReportChange] = &[
            ("explicit stop", |r| r.stopped = false),
            ("kill status", |r| r.returncode = Some(-libc::SIGTERM)),
            ("worker failure", |r| r.failure = Some("fixture".into())),
            ("cleanup failure", |r| {
                r.cleanup_failure = Some("fixture".into())
            }),
            ("cleanup proof", |r| r.cleanup_verified = false),
        ];
        for (condition, change) in changes {
            let mut report = accepted();
            change(&mut report);
            assert!(!report.stopped_cleanly(), "clean stop ignored {condition}");
        }
    }

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
    fn child_observes_incomplete_report_before_verified_finalization() {
        for saved in [true, false] {
            let (_owned, mut request) = fixture("printf 'unsaved'");
            let expected_path = request.report.clone().unwrap();
            if saved {
                request.argv = vec!["/bin/cat".into(), expected_path.to_str().unwrap().into()];
            } else {
                request.report = None;
            }
            let report = run(&request).unwrap();
            assert!(report.success(), "{report:?}");
            if saved {
                assert_eq!(
                    fs::read(&request.stdout).unwrap(),
                    b"{\"cleanup_verified\":false,\"failure\":\"supervision incomplete\"}\n"
                );
                let finalized = fs::read(&expected_path).unwrap();
                let decoded: Report = serde_json::from_slice(&finalized).unwrap();
                assert!(decoded.success(), "{decoded:?}");
                assert_eq!(decoded.pid, report.pid);
                assert_eq!(decoded.stdout_bytes, report.stdout_bytes);
                assert_eq!(decoded.stderr_bytes, report.stderr_bytes);
                assert_eq!(finalized.last(), Some(&b'\n'));
            } else {
                assert_eq!(fs::read(&request.stdout).unwrap(), b"unsaved");
                assert!(!expected_path.exists());
            }
        }
    }

    #[test]
    fn supervisor_handles_shared_and_distinct_output_paths() {
        for shared in [true, false] {
            let (_owned, mut request) = fixture("printf O; printf E >&2");
            if shared {
                request.stderr = request.stdout.clone();
            }
            let report = run(&request).unwrap();
            assert!(report.success(), "{report:?}");
            assert_eq!(
                (report.stdout_bytes, report.stderr_bytes),
                if shared { (2, 0) } else { (1, 1) }
            );
            let mut stdout = fs::read(&request.stdout).unwrap();
            if shared {
                stdout.sort_unstable();
                assert_eq!(stdout, b"EO");
            } else {
                assert_eq!(stdout, b"O");
                assert_eq!(fs::read(&request.stderr).unwrap(), b"E");
            }
        }
    }

    #[test]
    fn output_preflight_checks_the_report_before_acquiring_logs() {
        for kind in [
            "stdout alias",
            "stderr alias",
            "existing report",
            "missing parent",
        ] {
            let (owned, mut request) = fixture("exit 9");
            let marker = request.report.clone().unwrap();
            let expected = match kind {
                "stdout alias" => {
                    request.report = Some(request.stdout.clone());
                    "report aliases log"
                }
                "stderr alias" => {
                    request.report = Some(request.stderr.clone());
                    "report aliases log"
                }
                "existing report" => {
                    fs::write(&marker, b"preserve").unwrap();
                    "output already exists"
                }
                "missing parent" => {
                    request.report = Some(owned.path().join("missing").join("report.json"));
                    "output parent missing"
                }
                _ => unreachable!(),
            };
            assert!(
                run(&request).unwrap_err().to_string().contains(expected),
                "{kind}"
            );
            assert!(!request.stdout.exists());
            assert!(!request.stderr.exists());
            if kind == "existing report" {
                assert_eq!(fs::read(marker).unwrap(), b"preserve");
            } else {
                assert!(!marker.exists());
            }
        }
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
        assert!(
            report
                .failure
                .as_deref()
                .is_some_and(|text| text.contains("spawn child")),
            "spawn error was absent from the terminal report"
        );
        assert!(report.pid.is_none());
        assert!(report.cleanup_failure.is_none());
        assert!(report.cleanup_verified);
        let saved: Report =
            serde_json::from_slice(&fs::read(request.report.unwrap()).unwrap()).unwrap();
        assert!(saved.failure.is_some());
        assert!(saved.returncode.is_none());
        assert!(saved.cleanup_verified);
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
        for mode in ["timeout", "early", "delayed-early", "orphan", "malformed"] {
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
            // The recursive worker is sanitizer-instrumented too. Completion
            // cases need startup/exit headroom; the timeout control stays short.
            request.timeout_seconds = if mode == "timeout" { 3. } else { 30. };
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
                "early" | "delayed-early" => {
                    assert_eq!(report.returncode, Some(2), "{report:?}");
                    assert!(!report.timed_out, "{report:?}");
                }
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
            "early" | "delayed-early" => {
                if mode == "delayed-early" {
                    // Exercise completion beyond the old three-second bound,
                    // as instrumented worker startup/exit can take in CI.
                    thread::sleep(Duration::from_secs(4));
                }
                std::process::exit(2);
            }
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
