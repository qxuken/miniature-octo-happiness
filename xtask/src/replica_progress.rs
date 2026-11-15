//! Versioned, independently validated observations of the native snapshot CLI.
#[cfg(unix)]
use crate::supervise::{self, Request};
use crate::{delivery, support::Scratch};
use anyhow::{Context, Result, ensure};
use clap::Args;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
#[cfg(windows)]
use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::SystemTime,
    time::{Duration, Instant},
};
#[cfg(unix)]
use std::{
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    time::UNIX_EPOCH,
};

const SCHEMA: &str = "weekend-replica-progress";
const TEXT_MAX: u64 = 8192;
const PRIOR_MAX: u64 = 16_384;
const IMAGE_MAX: u64 = 64 * 1024 * 1024;
const BINARY_MAX: u64 = 256 * 1024 * 1024;

mod monitor;

#[derive(Args)]
#[group(id = "replica-progress-options", multiple = true)]
pub struct Options {
    #[arg(long)]
    binary: PathBuf,
    #[arg(long)]
    source: PathBuf,
    #[arg(long)]
    stream: String,
    #[arg(long)]
    peer: String,
    #[arg(long)]
    credentials: PathBuf,
    #[arg(long)]
    pin: PathBuf,
    #[arg(long, required_unless_present = "poll", conflicts_with = "poll")]
    ship: Option<PathBuf>,
    #[arg(long, required_unless_present = "ship")]
    poll: bool,
    #[arg(long, requires = "poll")]
    capture: Option<PathBuf>,
    #[arg(long)]
    previous: Option<PathBuf>,
    #[arg(long)]
    save: Option<PathBuf>,
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=3))]
    attempts: u8,
    #[arg(long, default_value_t = 110.0)]
    timeout: f64,
    #[arg(long, requires = "poll", conflicts_with_all = ["previous", "save"],
        value_parser = clap::value_parser!(u8).range(1..=64))]
    samples: Option<u8>,
    #[arg(long, requires = "samples", value_parser = clap::value_parser!(u64).range(100..=60_000))]
    interval_ms: Option<u64>,
}

// Missing nullable fields and explicit null have distinct meanings in v1.
fn required_option<'de, D, T>(input: D) -> std::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(input)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Ship,
    Poll,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Verified,
    Unknown,
    None,
    Durable,
    Empty,
    Unavailable,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Relation {
    ExactCapture,
    NewerThanCapture,
    BehindCapture,
    DigestConflict,
    NoCapture,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum PriorState {
    None,
    Consistent,
    Superseded,
    HistoricalOnly,
    Rejected,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Rejection {
    Timeout,
    CliFailure,
    ParseError,
    IdentityMismatch,
    CaptureInvalid,
    LocalRollback,
    FollowerRegression,
    DigestConflict,
    PriorInvalid,
}
type Observed<T> = std::result::Result<T, Rejection>;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Enrollment {
    stream: String,
    #[serde(deserialize_with = "required_option")]
    file_identity: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Local {
    state: State,
    #[serde(deserialize_with = "required_option")]
    revision: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    state: State,
    #[serde(deserialize_with = "required_option")]
    revision: Option<u64>,
    #[serde(deserialize_with = "required_option")]
    digest: Option<String>,
}
impl Progress {
    fn empty(state: State) -> Self {
        Self {
            state,
            revision: None,
            digest: None,
        }
    }
    fn observed(state: State, revision: u64, digest: String) -> Self {
        Self {
            state,
            revision: Some(revision),
            digest: Some(digest),
        }
    }
    fn valid(&self, state_with_payload: State, states: &[State]) -> bool {
        states.contains(&self.state)
            && if self.state == state_with_payload {
                self.revision.is_some() && self.digest.as_deref().is_some_and(|d| hex(d, 64))
            } else {
                self.revision.is_none() && self.digest.is_none()
            }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Prior {
    state: PriorState,
    #[serde(deserialize_with = "required_option")]
    observed_at: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: String,
    version: u8,
    observed_at: String,
    action: Action,
    #[serde(deserialize_with = "required_option")]
    cli_sha256: Option<String>,
    enrollment: Enrollment,
    local: Local,
    capture: Progress,
    follower: Progress,
    relation: Relation,
    prior: Prior,
    #[serde(deserialize_with = "required_option")]
    error: Option<Rejection>,
}
impl Record {
    fn new(stream: &str, action: Action) -> Result<Self> {
        Ok(Self {
            schema: SCHEMA.to_owned(),
            version: 1,
            observed_at: timestamp()?,
            action,
            cli_sha256: None,
            enrollment: Enrollment {
                stream: stream.to_owned(),
                file_identity: None,
            },
            local: Local {
                state: State::Unknown,
                revision: None,
            },
            capture: Progress::empty(if action == Action::Ship {
                State::Unknown
            } else {
                State::None
            }),
            follower: Progress::empty(State::Unknown),
            relation: Relation::Unknown,
            prior: Prior {
                state: PriorState::None,
                observed_at: None,
            },
            error: None,
        })
    }
    fn relation(&self) -> Relation {
        if self.follower.state != State::Durable {
            return Relation::Unknown;
        }
        if self.capture.state != State::Verified {
            return Relation::NoCapture;
        }
        use std::cmp::Ordering::*;
        match self.follower.revision.cmp(&self.capture.revision) {
            Greater => Relation::NewerThanCapture,
            Less => Relation::BehindCapture,
            Equal if self.follower.digest == self.capture.digest => Relation::ExactCapture,
            Equal => Relation::DigestConflict,
        }
    }
    fn valid(&self) -> bool {
        let local_valid = match self.local.state {
            State::Verified => self.local.revision.is_some(),
            State::Unknown => self.local.revision.is_none(),
            _ => false,
        };
        if self.schema != SCHEMA
            || self.version != 1
            || !valid_timestamp(&self.observed_at)
            || self.cli_sha256.as_deref().is_some_and(|d| !hex(d, 64))
            || !stream_id(&self.enrollment.stream)
            || self
                .enrollment
                .file_identity
                .as_deref()
                .is_some_and(|v| !stream_id(v))
            || !local_valid
            || !self.capture.valid(
                State::Verified,
                &[State::Verified, State::None, State::Unknown],
            )
            || !self.follower.valid(
                State::Durable,
                &[
                    State::Durable,
                    State::Empty,
                    State::Unknown,
                    State::Unavailable,
                    State::Error,
                ],
            )
            || self
                .prior
                .observed_at
                .as_deref()
                .is_some_and(|v| !valid_timestamp(v))
        {
            return false;
        }
        let expected = self.relation();
        let file = self.enrollment.file_identity.is_some();
        let identity_needed = self.local.revision.is_some()
            || self.capture.revision.is_some()
            || self.follower.revision.is_some();
        let relation_ok =
            self.relation == expected || self.error.is_some() && self.relation == Relation::Unknown;
        (file || !identity_needed)
            && relation_ok
            && (self.error.is_some()
                || self.cli_sha256.is_some()
                    && file
                    && self.local.state == State::Verified
                    && matches!(self.follower.state, State::Durable | State::Empty)
                    && expected != Relation::DigestConflict
                    && self.capture.revision <= self.local.revision
                    && self.follower.revision <= self.local.revision)
    }

    fn reject_prior(&mut self, kind: Rejection) -> Observed<()> {
        self.prior.state = PriorState::Rejected;
        Err(kind)
    }
    fn reconcile(&mut self, prior: Option<&Self>) -> Observed<()> {
        if self.capture.revision > self.local.revision {
            return Err(Rejection::LocalRollback);
        }
        if self.follower.revision > self.local.revision {
            return Err(Rejection::FollowerRegression);
        }
        self.relation = self.relation();
        if self.relation == Relation::DigestConflict {
            return Err(Rejection::DigestConflict);
        }
        if let Some(prior) = prior {
            self.prior = Prior {
                state: PriorState::HistoricalOnly,
                observed_at: Some(prior.observed_at.clone()),
            };
            if prior.enrollment != self.enrollment {
                return self.reject_prior(Rejection::IdentityMismatch);
            }
            if prior.local.revision > self.local.revision {
                return self.reject_prior(Rejection::LocalRollback);
            }
            if prior.follower.state == State::Durable
                && matches!(self.follower.state, State::Durable | State::Empty)
            {
                use std::cmp::Ordering::*;
                self.prior.state = match self.follower.revision.cmp(&prior.follower.revision) {
                    Less => return self.reject_prior(Rejection::FollowerRegression),
                    Equal if self.follower.digest != prior.follower.digest => {
                        return self.reject_prior(Rejection::DigestConflict);
                    }
                    Equal => PriorState::Consistent,
                    Greater => PriorState::Superseded,
                };
            }
        }
        Ok(())
    }
}

fn hex(value: &str, digits: usize) -> bool {
    value.len() == digits
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn stream_id(value: &str) -> bool {
    hex(value, 32) && value.bytes().any(|c| c != b'0')
}

fn valid_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 27
        || [
            (4, b'-'),
            (7, b'-'),
            (10, b'T'),
            (13, b':'),
            (16, b':'),
            (19, b'.'),
            (26, b'Z'),
        ]
        .into_iter()
        .any(|(i, expected)| bytes[i] != expected)
    {
        return false;
    }
    let value = |range: std::ops::Range<usize>| -> Option<u32> {
        let digits = &bytes[range];
        digits.iter().try_fold(0_u32, |value, b| {
            b.is_ascii_digit().then(|| value * 10 + u32::from(b - b'0'))
        })
    };
    let [
        Some(year),
        Some(month),
        Some(day),
        Some(hour),
        Some(minute),
        Some(second),
        Some(_),
    ] = [
        value(0..4),
        value(5..7),
        value(8..10),
        value(11..13),
        value(14..16),
        value(17..19),
        value(20..26),
    ]
    else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        2 => 28 + u32::from(leap),
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    year > 0 && (1..=max_day).contains(&day) && hour < 24 && minute < 60 && second < 60
}

#[cfg(unix)]
fn timestamp() -> Result<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let seconds = libc::time_t::try_from(now.as_secs())?;
    let mut parts = std::mem::MaybeUninit::<libc::tm>::uninit();
    // Both pointers remain valid during gmtime_r. Read the result only on success.
    ensure!(
        !unsafe { libc::gmtime_r(&seconds, parts.as_mut_ptr()) }.is_null(),
        "UTC clock unavailable"
    );
    let parts = unsafe { parts.assume_init() };
    let value = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        parts.tm_year + 1900,
        parts.tm_mon + 1,
        parts.tm_mday,
        parts.tm_hour,
        parts.tm_min,
        parts.tm_sec,
        now.subsec_micros()
    );
    ensure!(valid_timestamp(&value), "invalid UTC clock observation");
    Ok(value)
}

#[cfg(windows)]
fn timestamp() -> Result<String> {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetSystemTime};
    let mut now = SYSTEMTIME::default();
    // SAFETY: Windows writes the current UTC fields into the live output.
    unsafe { GetSystemTime(&mut now) };
    let value = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        now.wYear,
        now.wMonth,
        now.wDay,
        now.wHour,
        now.wMinute,
        now.wSecond,
        u32::from(now.wMilliseconds) * 1000,
    );
    ensure!(valid_timestamp(&value), "invalid UTC clock observation");
    Ok(value)
}

fn open_read_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NONBLOCK);
    #[cfg(windows)]
    options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(path)
}

fn prior_record(path: &Path) -> Observed<Record> {
    let fallible = || -> Result<Record> {
        let file = open_read_file(path)?;
        ensure!(
            file_signature(&file)?.len <= PRIOR_MAX,
            "oversized prior file"
        );
        let mut data = Vec::new();
        file.take(PRIOR_MAX + 1).read_to_end(&mut data)?;
        ensure!(data.len() as u64 <= PRIOR_MAX, "oversized prior file");
        let record: Record = serde_json::from_value(delivery::json(&data)?)?;
        ensure!(record.valid(), "invalid prior observation");
        Ok(record)
    };
    fallible().map_err(|_| Rejection::PriorInvalid)
}

fn revision(value: &str) -> Observed<u64> {
    if value.is_empty()
        || value.len() > 20
        || (value.starts_with('0') && value != "0")
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Rejection::ParseError);
    }
    value.parse().map_err(|_| Rejection::ParseError)
}

fn local_identity<'a>(line: &'a str, prefix: &str) -> Observed<(u64, &'a str, &'a str)> {
    let row = line.strip_prefix(prefix).ok_or(Rejection::ParseError)?;
    let (rev, rest) = row.split_once(" stream=").ok_or(Rejection::ParseError)?;
    let (stream, identity) = rest
        .split_once(" file_identity=")
        .ok_or(Rejection::ParseError)?;
    if !hex(stream, 32) || !hex(identity, 32) {
        return Err(Rejection::ParseError);
    }
    Ok((revision(rev)?, stream, identity))
}

fn follower_receipt(line: &str, expected: &str) -> Observed<Progress> {
    if line == "follower empty" {
        return Ok(Progress::empty(State::Empty));
    }
    let row = line
        .strip_prefix("follower durable_revision=")
        .ok_or(Rejection::ParseError)?;
    let (rev, rest) = row.split_once(" stream=").ok_or(Rejection::ParseError)?;
    let (stream, digest) = rest.split_once(" digest=").ok_or(Rejection::ParseError)?;
    if !hex(stream, 32) || !hex(digest, 64) {
        return Err(Rejection::ParseError);
    }
    if stream != expected {
        return Err(Rejection::IdentityMismatch);
    }
    Ok(Progress::observed(
        State::Durable,
        revision(rev)?,
        digest.to_owned(),
    ))
}

#[derive(PartialEq, Eq)]
struct FileSignature {
    identity: (u64, u64),
    len: u64,
    modified: SystemTime,
}

fn file_signature(file: &File) -> Result<FileSignature> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && !delivery::reparse(&metadata),
        "regular observation file required"
    );
    #[cfg(unix)]
    let identity = (metadata.dev(), metadata.ino());
    #[cfg(windows)]
    let identity = {
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_TYPE_DISK, GetFileInformationByHandle, GetFileType,
        };
        let handle = file.as_raw_handle();
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the file owns a live borrowed handle and the output is writable.
        unsafe {
            ensure!(GetFileType(handle) == FILE_TYPE_DISK, "disk file required");
            ensure!(
                GetFileInformationByHandle(handle, &mut info) != 0,
                "file identity unavailable"
            );
        }
        (
            u64::from(info.dwVolumeSerialNumber),
            (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        )
    };
    Ok(FileSignature {
        identity,
        len: metadata.len(),
        modified: metadata.modified()?,
    })
}

fn digest_file(
    path: &Path,
    maximum: u64,
    deadline: Instant,
    invalid: Rejection,
) -> Observed<String> {
    let mut file = open_read_file(path).map_err(|_| invalid)?;
    let before = file_signature(&file).map_err(|_| invalid)?;
    if before.len > maximum {
        return Err(invalid);
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65_536];
    let mut size = 0;
    loop {
        if Instant::now() >= deadline {
            return Err(Rejection::Timeout);
        }
        let n = file.read(&mut buffer).map_err(|_| invalid)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > before.len {
            return Err(invalid);
        }
        hash.update(&buffer[..n]);
    }
    let after = file_signature(&file).map_err(|_| invalid)?;
    let reopened =
        file_signature(&open_read_file(path).map_err(|_| invalid)?).map_err(|_| invalid)?;
    if size != before.len || before != after || before != reopened {
        return Err(invalid);
    }
    Ok(format!("{:x}", hash.finalize()))
}

struct Probe<'a> {
    options: &'a Options,
    directory: &'a Path,
    deadline: Instant,
    command: u64,
}
impl Probe<'_> {
    fn invoke(&mut self, args: &[&OsStr]) -> Observed<(i32, Vec<String>)> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Rejection::Timeout);
        }
        let argv = std::iter::once(self.options.binary.as_os_str())
            .chain(args.iter().copied())
            .map(OsStr::to_owned)
            .collect::<Vec<_>>();
        self.command += 1;
        let stdout = self.directory.join(format!("{}.stdout", self.command));
        let stderr = self.directory.join(format!("{}.stderr", self.command));
        let code = Self::run_cli(&argv, &stdout, &stderr, remaining)?;
        let text = fs::read(stdout).map_err(|_| Rejection::CliFailure)?;
        if !text.is_ascii() {
            return Err(Rejection::ParseError);
        }
        let lines = std::str::from_utf8(&text)
            .map_err(|_| Rejection::ParseError)?
            .lines()
            .map(ToOwned::to_owned)
            .collect();
        Ok((code, lines))
    }

    #[cfg(unix)]
    fn run_cli(
        argv: &[OsString],
        stdout: &Path,
        stderr: &Path,
        remaining: Duration,
    ) -> Observed<i32> {
        let argv = argv
            .iter()
            .map(|arg| {
                arg.to_str()
                    .map(ToOwned::to_owned)
                    .ok_or(Rejection::CliFailure)
            })
            .collect::<Observed<_>>()?;
        let result = supervise::run(&Request {
            argv,
            cwd: None,
            env: None,
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
            report: None,
            timeout_seconds: remaining.as_secs_f64(),
            maximum_bytes: TEXT_MAX,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        })
        .map_err(|_| Rejection::CliFailure)?;
        if result.timed_out {
            return Err(Rejection::Timeout);
        }
        if !result.cleanup_verified || result.cleanup_failure.is_some() || result.cancelled {
            return Err(Rejection::CliFailure);
        }
        if result.failure.as_deref() == Some("command output limit") {
            return Err(Rejection::ParseError);
        }
        let code = result.returncode.ok_or(Rejection::CliFailure)?;
        if code == 0 && !result.success()
            || code != 0 && result.failure.as_deref() != Some(&format!("child exit Some({code})"))
        {
            return Err(Rejection::CliFailure);
        }
        Ok(code)
    }

    #[cfg(windows)]
    fn run_cli(
        argv: &[OsString],
        stdout: &Path,
        stderr: &Path,
        remaining: Duration,
    ) -> Observed<i32> {
        use crate::trace_process::{self, Request};
        let cwd = std::env::current_dir().map_err(|_| Rejection::CliFailure)?;
        let env = std::env::vars_os().collect();
        let result = trace_process::run(&Request {
            argv,
            cwd: &cwd,
            env: &env,
            stdout,
            stderr,
            timeout: remaining.as_secs_f64(),
            maximum: TEXT_MAX,
        })
        .map_err(|_| Rejection::CliFailure)?;
        if !result.cleanup_verified || result.stopped {
            return Err(Rejection::CliFailure);
        }
        if result.timed_out {
            return Err(Rejection::Timeout);
        }
        if result.output_exceeded && result.failure.as_deref() == Some("command output limit") {
            return Err(Rejection::ParseError);
        }
        if !result.qualifies(true) {
            return Err(Rejection::CliFailure);
        }
        i32::try_from(result.exit.ok_or(Rejection::CliFailure)?).map_err(|_| Rejection::CliFailure)
    }

    fn describe(&mut self, path: &Path) -> Observed<(u64, String)> {
        let stream = self.options.stream.as_str();
        let (status, lines) =
            self.invoke(&["describe".as_ref(), path.as_os_str(), stream.as_ref()])?;
        if status != 0 {
            return Err(Rejection::CliFailure);
        }
        let [local, initialization] = lines.as_slice() else {
            return Err(Rejection::ParseError);
        };
        let (revision, got_stream, identity) = local_identity(local, "local verified_revision=")?;
        if got_stream != stream
            || !stream_id(identity)
            || *initialization
                != format!("weekend-replicate follower-init EMPTY_DIR {stream} {identity}")
        {
            return Err(Rejection::ParseError);
        }
        Ok((revision, identity.to_owned()))
    }

    fn capture(&mut self, path: &Path, identity: &str) -> Observed<Progress> {
        let (revision, observed_identity) = self.describe(path)?;
        if observed_identity != identity {
            return Err(Rejection::IdentityMismatch);
        }
        let digest = digest_file(path, IMAGE_MAX, self.deadline, Rejection::CaptureInvalid)?;
        Ok(Progress::observed(State::Verified, revision, digest))
    }

    fn check(&mut self, record: &mut Record, previous: Option<&Record>) -> Observed<()> {
        let options = self.options;
        record.cli_sha256 = Some(digest_file(
            &options.binary,
            BINARY_MAX,
            self.deadline,
            Rejection::CliFailure,
        )?);
        let saved = if let Some(path) = &options.previous {
            match prior_record(path) {
                Ok(prior) => Some(prior),
                Err(reason) => return record.reject_prior(reason),
            }
        } else {
            None
        };
        let prior = previous.or(saved.as_ref());
        if let Some(prior) = prior {
            record.prior = Prior {
                state: PriorState::HistoricalOnly,
                observed_at: Some(prior.observed_at.clone()),
            };
        }
        let (rev, identity) = self.describe(&options.source)?;
        record.enrollment.file_identity = Some(identity.clone());
        record.local = Local {
            state: State::Verified,
            revision: Some(rev),
        };
        if prior.is_some_and(|p| p.enrollment != record.enrollment) {
            return record.reject_prior(Rejection::IdentityMismatch);
        }
        if let Some(path) = &options.capture {
            record.capture = self.capture(path, &identity)?;
        }
        let mut call = vec![
            if options.ship.is_some() {
                "ship".as_ref()
            } else {
                "poll".as_ref()
            },
            options.source.as_os_str(),
            options.stream.as_ref(),
        ];
        if let Some(path) = &options.ship {
            call.push(path.as_os_str());
        }
        call.extend([
            options.peer.as_ref(),
            options.credentials.as_os_str(),
            options.pin.as_os_str(),
        ]);
        let attempts = OsString::from(options.attempts.to_string());
        if options.ship.is_some() {
            call.extend(["--attempts".as_ref(), attempts.as_os_str()]);
        }
        let (status, lines) = self.invoke(&call)?;
        if let Some(local) = lines.first() {
            if let Some(export) = &options.ship {
                let (rev, stream, file) = local_identity(local, "local captured_revision=")?;
                if stream != options.stream || file != identity {
                    return Err(Rejection::IdentityMismatch);
                }
                record.capture = self.capture(export, &identity)?;
                if record.capture.revision != Some(rev) {
                    return Err(Rejection::CaptureInvalid);
                }
                record.local.revision = Some(rev);
            } else {
                let rev = local
                    .strip_prefix("local verified_revision=")
                    .ok_or(Rejection::ParseError)?;
                record.local.revision = Some(revision(rev)?);
            }
        }
        if status != 0 {
            return Err(Rejection::CliFailure);
        }
        let [_, follower] = lines.as_slice() else {
            return Err(Rejection::ParseError);
        };
        record.follower = follower_receipt(follower, &options.stream)?;
        let (latest, latest_identity) = self.describe(&options.source)?;
        if latest_identity != identity || Some(latest) < record.local.revision {
            return Err(Rejection::IdentityMismatch);
        }
        record.reconcile(prior)
    }
}

fn observe(options: &Options) -> Result<Record> {
    observe_with_prior(options, None)
}

fn observe_with_prior(options: &Options, prior: Option<&Record>) -> Result<Record> {
    let action = if options.ship.is_some() {
        Action::Ship
    } else {
        Action::Poll
    };
    let mut record = Record::new(&options.stream, action)?;
    let deadline = Instant::now() + Duration::from_secs_f64(options.timeout);
    let result = (|| -> Observed<()> {
        let scratch = Scratch::new(&std::env::temp_dir(), "replica-progress")
            .map_err(|_| Rejection::CliFailure)?;
        Probe {
            options,
            directory: scratch.path(),
            deadline,
            command: 0,
        }
        .check(&mut record, prior)
    })();
    if let Err(reason) = result {
        record.error = Some(reason);
        if record.follower.state != State::Durable {
            record.follower = Progress::empty(if action == Action::Ship {
                State::Unknown
            } else {
                State::Unavailable
            });
        }
    }
    record.observed_at = timestamp()?;
    ensure!(record.valid(), "observer produced an invalid record");
    Ok(record)
}

fn admit(options: &Options) -> Result<()> {
    ensure!(
        stream_id(&options.stream),
        "nonzero stream requires 32 lowercase hex digits"
    );
    ensure!(
        (1..=3).contains(&options.attempts) && (1.0..=600.0).contains(&options.timeout),
        "attempts 1..3 and timeout 1..600 required"
    );
    ensure!(
        options.poll != options.ship.is_some() && (options.capture.is_none() || options.poll),
        "choose ship or poll; capture belongs to poll"
    );
    if let Some(samples) = options.samples {
        ensure!(
            (1..=64).contains(&samples)
                && options.poll
                && options.previous.is_none()
                && options.save.is_none()
                && options
                    .interval_ms
                    .is_none_or(|ms| (100..=60_000).contains(&ms)),
            "sampled polling requires 1..64 samples and a 100..60000 millisecond interval"
        );
    } else {
        ensure!(options.interval_ms.is_none(), "interval requires samples");
    }
    if let Some(path) = &options.save {
        let parent = path.parent().context("observation save parent required")?;
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        ensure!(
            fs::symlink_metadata(path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                && parent.is_dir(),
            "save requires a fresh destination"
        );
    }
    Ok(())
}

fn save(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub fn run(options: Options) -> Result<()> {
    admit(&options)?;
    if let Some(samples) = options.samples {
        return monitor::run(&options, samples, io::stdout().lock());
    }
    let record = observe(&options)?;
    let output = serde_json::to_value(&record)?;
    let encoded = format!("{output}\n");
    if let Some(path) = &options.save {
        save(path, encoded.as_bytes())?;
    }
    print!("{encoded}");
    ensure!(
        record.error.is_none(),
        "replica observation failed: {}",
        output["error"]
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::assurance::Campaign;
    #[cfg(windows)]
    use crate::windows_compatibility::Campaign;
    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};

    const STREAM: &str = "11111111111111111111111111111111";
    const IDENTITY: &str = "22222222222222222222222222222222";

    fn sample() -> Record {
        let mut record = Record::new(STREAM, Action::Poll).unwrap();
        record.observed_at = "2026-11-13T00:00:00.000000Z".to_owned();
        record.cli_sha256 = Some("12".repeat(32));
        record.enrollment.file_identity = Some(IDENTITY.to_owned());
        record.local = Local {
            state: State::Verified,
            revision: Some(4),
        };
        record.capture = Progress::observed(State::Verified, 3, "33".repeat(32));
        record.follower = Progress::observed(State::Durable, 3, "33".repeat(32));
        record.relation = Relation::ExactCapture;
        record
    }

    #[test]
    fn relations_and_prior_never_fabricate_durable_progress() {
        let prior = sample();
        for (revision, expected) in [
            (2, Relation::BehindCapture),
            (3, Relation::ExactCapture),
            (4, Relation::NewerThanCapture),
        ] {
            let mut actual = prior.clone();
            actual.follower.revision = Some(revision);
            actual.reconcile(None).unwrap();
            assert_eq!(actual.relation, expected);
            assert!(actual.valid());
        }
        let mut unknown = sample();
        unknown.follower = Progress::empty(State::Unknown);
        unknown.reconcile(Some(&prior)).unwrap();
        assert_eq!(unknown.relation, Relation::Unknown);
        assert_eq!(unknown.prior.state, PriorState::HistoricalOnly);
        assert!(unknown.follower.revision.is_none());
        unknown.follower = Progress::observed(State::Durable, 2, "33".repeat(32));
        assert_eq!(
            unknown.reconcile(Some(&prior)),
            Err(Rejection::FollowerRegression)
        );
        let mut wrong = sample();
        wrong.follower.digest = Some("44".repeat(32));
        assert_eq!(wrong.reconcile(None), Err(Rejection::DigestConflict));
        wrong.follower.revision = Some(5);
        assert_eq!(wrong.reconcile(None), Err(Rejection::FollowerRegression));
    }

    #[test]
    fn saved_schema_validates_nullable_fields_identifiers_timestamps_and_relation() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-record").unwrap();
        let path = tmp.path().join("record.json");
        let valid = serde_json::to_value(sample()).unwrap();
        fs::write(&path, valid.to_string()).unwrap();
        assert_eq!(prior_record(&path).unwrap(), sample());
        for (field, replacement) in [
            ("/version", serde_json::json!(true)),
            ("/observed_at", serde_json::json!("today")),
            ("/relation", serde_json::json!("newer_than_capture")),
            ("/cli_sha256", serde_json::json!(7)),
            ("/enrollment/stream", serde_json::json!(7)),
            ("/enrollment/stream", serde_json::json!("0".repeat(32))),
            (
                "/enrollment/file_identity",
                serde_json::json!("0".repeat(32)),
            ),
            ("/enrollment/file_identity", serde_json::json!(7)),
            ("/capture/digest", serde_json::json!(7)),
            ("/follower/digest", serde_json::json!(7)),
            ("/prior/observed_at", serde_json::json!("yesterday")),
            (
                "/prior/observed_at",
                serde_json::json!("2026-99-13T00:00:00.000000Z"),
            ),
            (
                "/prior/observed_at",
                serde_json::json!("1900-02-29T00:00:00.000000Z"),
            ),
        ] {
            let mut malformed = valid.clone();
            *malformed.pointer_mut(field).unwrap() = replacement;
            fs::write(&path, malformed.to_string()).unwrap();
            assert_eq!(prior_record(&path), Err(Rejection::PriorInvalid), "{field}");
        }
        for missing in ["cli_sha256", "error", "prior"] {
            let mut malformed = valid.clone();
            malformed.as_object_mut().unwrap().remove(missing);
            fs::write(&path, malformed.to_string()).unwrap();
            assert_eq!(
                prior_record(&path),
                Err(Rejection::PriorInvalid),
                "{missing}"
            );
        }
        let mut extra = valid.clone();
        extra["extra"] = serde_json::json!(1);
        for contents in [
            extra.to_string(),
            "{\"version\":1,\"version\":2}".into(),
            "[".repeat(1200) + "0" + &"]".repeat(1200),
        ] {
            fs::write(&path, contents).unwrap();
            assert_eq!(prior_record(&path), Err(Rejection::PriorInvalid));
        }
        assert!(valid_timestamp("2000-02-29T23:59:59.123456Z"));
        assert!(follower_receipt("follower empty garbage", STREAM).is_err());
        assert!(
            follower_receipt(
                &format!(
                    "follower durable_revision=01 stream={STREAM} digest={}",
                    "33".repeat(32)
                ),
                STREAM
            )
            .is_err()
        );
        assert_eq!(revision(&u64::MAX.to_string()).unwrap(), u64::MAX);
        assert_eq!(revision("18446744073709551616"), Err(Rejection::ParseError));
    }

    fn options(root: &Path) -> Options {
        Options {
            binary: root.join("bin"),
            source: root.join("source.wdb"),
            stream: STREAM.into(),
            peer: "127.0.0.1:1".into(),
            credentials: root.join("SECRET"),
            pin: root.join("PRIVATE"),
            ship: None,
            poll: true,
            capture: None,
            previous: None,
            save: None,
            attempts: 1,
            timeout: 3.,
            samples: None,
            interval_ms: None,
        }
    }

    #[test]
    fn sampled_progress_keeps_durable_age_and_catch_up_separate_from_failed_probes() {
        let poll = |local, durable: Option<u64>, error| {
            let mut record = sample();
            record.local.revision = Some(local);
            record.capture = Progress::empty(State::None);
            record.follower = durable.map_or_else(
                || Progress::empty(State::Empty),
                |revision| Progress::observed(State::Durable, revision, "33".repeat(32)),
            );
            record.error = error;
            record.relation = record.relation();
            assert!(record.valid());
            record
        };
        let mut monitor = monitor::Monitor::default();
        let mut measure = |number, ms, record| {
            monitor.sample(number, Duration::from_millis(ms), Duration::ZERO, record)
        };
        let empty = measure(1, 10, poll(3, None, None));
        assert!(empty["revision_gap"].is_null());
        assert!(empty["last_durable"].is_null());
        assert_eq!(empty["waiting_for"]["revision"], 3);
        let behind = measure(2, 20, poll(4, Some(3), None));
        assert_eq!(behind["revision_gap"], 1);
        assert_eq!(behind["caught_up"]["observed_after_ms"], 10);
        assert_eq!(behind["waiting_for"]["revision"], 4);

        // An authenticated response can still be rejected by reconciliation.
        let conflict = measure(3, 35, poll(4, Some(4), Some(Rejection::DigestConflict)));
        assert_eq!(conflict["accepted"], false);
        assert!(conflict["revision_gap"].is_null());
        assert!(conflict["waiting_for"].is_null());
        assert!(conflict["caught_up"].is_null());
        assert_eq!(conflict["last_durable"]["revision"], 3);
        assert_eq!(conflict["last_durable"]["age_ms"], 15);

        let advanced = measure(4, 60, poll(5, Some(4), None));
        assert_eq!(advanced["revision_gap"], 1);
        assert_eq!(advanced["caught_up"]["revision"], 4);
        assert_eq!(advanced["caught_up"]["observed_after_ms"], 40);
        assert_eq!(advanced["waiting_for"]["revision"], 5);
        let current = measure(5, 80, poll(5, Some(5), None));
        assert!(current["waiting_for"].is_null());
        assert_eq!(current["revision_gap"], 0);
        assert_eq!(current["last_durable"]["age_ms"], 0);
    }
    #[cfg(unix)]
    fn script(binary: &Path, content: &str) {
        fs::write(binary, format!("#!/bin/sh\n{content}\n")).unwrap();
        fs::set_permissions(binary, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[cfg(unix)]
    fn describe_rows(revision: &str, identity: &str) -> String {
        format!(
            "local verified_revision={revision} stream={STREAM} file_identity={identity}\nweekend-replicate follower-init EMPTY_DIR {STREAM} {identity}\n"
        )
    }

    #[cfg(unix)]
    #[test]
    fn sampled_poll_rejects_regression_after_an_unavailable_sample_and_continues() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-monitor").unwrap();
        let mut options = options(tmp.path());
        options.samples = Some(4);
        options.interval_ms = Some(100);
        script(
            &options.binary,
            &format!(
                r#"
if [ "$1" = describe ]; then
  printf '%s' '{}'
  exit 0
fi
counter="$(dirname "$0")/poll-count"
count=0
if [ -f "$counter" ]; then read -r count < "$counter"; fi
count=$((count + 1))
printf '%s\n' "$count" > "$counter"
printf '%s\n' 'local verified_revision=4'
case "$count" in
  1) next=3 ;;
  3) next=2 ;;
  4) next=4 ;;
  *) exit 1 ;;
esac
printf 'follower durable_revision=%s stream={STREAM} digest={}\n' "$next"
"#,
                describe_rows("4", IDENTITY),
                "33".repeat(32)
            ),
        );
        admit(&options).unwrap();
        let mut output = Vec::new();
        monitor::run(&options, 4, &mut output).unwrap();
        let rows: Vec<_> = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .map(|line| delivery::json(line.as_bytes()).unwrap())
            .collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["revision_gap"], 1);
        assert_eq!(rows[1]["observation"]["error"], "cli_failure");
        assert_eq!(rows[2]["observation"]["error"], "follower_regression");
        for stale in &rows[1..3] {
            assert_eq!(stale["accepted"], false);
            assert_eq!(stale["last_durable"]["revision"], 3);
            assert!(stale["last_durable"]["age_ms"].as_u64().unwrap() >= 100);
            assert!(stale["revision_gap"].is_null());
        }
        assert_eq!(rows[3]["accepted"], true);
        assert_eq!(rows[3]["caught_up"]["revision"], 4);
        assert_eq!(rows[3]["last_durable"]["age_ms"], 0);

        output.clear();
        assert!(monitor::run(&options, 1, &mut output).is_err());
        let failed = delivery::json(&output).unwrap();
        assert_eq!(failed["accepted"], false);
        assert!(failed["last_durable"].is_null());
    }

    #[cfg(unix)]
    #[test]
    fn invalid_identity_and_oversized_revision_reject_before_poll() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-parse").unwrap();
        let options = options(tmp.path());
        for (revision, identity) in [
            ("4".to_owned(), "0".repeat(32)),
            ("1".repeat(5000), IDENTITY.to_owned()),
        ] {
            let output = describe_rows(&revision, &identity);
            script(
                &options.binary,
                &format!(
                    "if [ \"$1\" = describe ]; then printf '%s' '{output}'; else touch \"$(dirname \"$0\")/polled\"; fi"
                ),
            );
            let record = observe(&options).unwrap();
            assert_eq!(record.error, Some(Rejection::ParseError));
            assert!(record.enrollment.file_identity.is_none());
            assert!(record.local.revision.is_none());
            assert!(record.follower.revision.is_none());
            assert!(!tmp.path().join("polled").exists());
        }
    }

    #[test]
    fn excessive_output_is_rejected_before_the_child_deadline() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-output").unwrap();
        let options = options(tmp.path());
        #[cfg(unix)]
        script(&options.binary, "head -c 9000 /dev/zero; exec sleep 30");
        #[cfg(unix)]
        let args: &[&OsStr] = &[];
        #[cfg(windows)]
        let options = Options {
            binary: "cmd.exe".into(),
            ..options
        };
        #[cfg(windows)]
        let args = &[
            "/D".as_ref(),
            "/Q".as_ref(),
            "/C".as_ref(),
            "for /L %i in (1,1,1000000) do @echo abcdefghijklmnopqrstuvwxyz".as_ref(),
        ];
        let started = Instant::now();
        let mut probe = Probe {
            options: &options,
            directory: tmp.path(),
            deadline: started + Duration::from_secs(10),
            command: 0,
        };
        assert_eq!(probe.invoke(args), Err(Rejection::ParseError));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(
            fs::metadata(tmp.path().join("1.stdout")).unwrap().len(),
            TEXT_MAX
        );
    }

    #[cfg(unix)]
    #[test]
    fn observer_log_setup_failure_launches_no_child() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-output").unwrap();
        let options = options(tmp.path());
        script(&options.binary, "touch \"$(dirname \"$0\")/invoked\"");
        let stdout = tmp.path().join("1.stdout");
        fs::write(&stdout, b"keep").unwrap();
        let mut probe = Probe {
            options: &options,
            directory: tmp.path(),
            deadline: Instant::now() + Duration::from_secs(5),
            command: 0,
        };
        assert_eq!(probe.invoke(&[]), Err(Rejection::CliFailure));
        assert_eq!(fs::read(stdout).unwrap(), b"keep");
        assert!(!tmp.path().join("invoked").exists());
        assert!(!tmp.path().join("1.stderr").exists());
    }

    #[cfg(unix)]
    #[test]
    fn timeout_keeps_valid_prior_as_historical_only() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-timeout").unwrap();
        let mut options = options(tmp.path());
        let prior = tmp.path().join("prior.json");
        fs::write(&prior, serde_json::to_vec(&sample()).unwrap()).unwrap();
        options.previous = Some(prior);
        options.timeout = 1.5;
        let output = describe_rows("4", IDENTITY);
        script(
            &options.binary,
            &format!(
                "if [ \"$1\" = describe ]; then printf '%s' '{output}'; else exec sleep 30; fi"
            ),
        );
        let record = observe(&options).unwrap();
        assert_eq!(record.error, Some(Rejection::Timeout));
        assert_eq!(record.prior.state, PriorState::HistoricalOnly);
        assert_eq!(
            record.prior.observed_at.as_deref(),
            Some(sample().observed_at.as_str())
        );
        assert_eq!(record.follower, Progress::empty(State::Unavailable));
    }

    #[test]
    fn existing_or_dangling_observation_rejects_before_replication() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-save").unwrap();
        let mut options = options(tmp.path());
        let existing = tmp.path().join("prior.json");
        fs::write(&existing, b"keep").unwrap();
        #[cfg(unix)]
        let link = tmp.path().join("dangling");
        #[cfg(unix)]
        symlink(tmp.path().join("missing"), &link).unwrap();
        #[cfg(unix)]
        let destinations = [&existing, &link];
        #[cfg(windows)]
        let destinations = [&existing];
        for path in destinations {
            options.save = Some(path.to_owned());
            assert!(admit(&options).is_err());
        }
        assert_eq!(fs::read(existing).unwrap(), b"keep");
        assert!(!tmp.path().join("invoked").exists());
    }

    #[cfg(unix)]
    #[test]
    fn failed_ship_preserves_verified_local_capture_without_acknowledgment() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-ship").unwrap();
        let mut options = options(tmp.path());
        // This exercises a failed reply after several real child invocations.
        // Keep sanitizer startup overhead out of the result being asserted.
        options.timeout = 90.;
        options.ship = Some(tmp.path().join("export"));
        options.poll = false;
        let output = describe_rows("3", IDENTITY);
        script(
            &options.binary,
            &format!(
                "if [ \"$1\" = describe ]; then printf '%s' '{output}'; else printf sample > \"$4\"; printf '%s\\n' 'local captured_revision=3 stream={STREAM} file_identity={IDENTITY}'; exit 1; fi"
            ),
        );
        let record = observe(&options).unwrap();
        assert_eq!(record.error, Some(Rejection::CliFailure));
        assert_eq!(record.capture.state, State::Verified);
        assert_eq!(record.capture.revision, Some(3));
        assert_eq!(record.follower, Progress::empty(State::Unknown));
        let encoded = serde_json::to_string(&record).unwrap();
        assert!(!encoded.contains("SECRET") && !encoded.contains("PRIVATE"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_child_deadline_rejects_without_using_transport_output() {
        let tmp = Scratch::new(&std::env::temp_dir(), "replica-timeout").unwrap();
        let options = Options {
            binary: "cmd.exe".into(),
            ..options(tmp.path())
        };
        let mut probe = Probe {
            options: &options,
            directory: tmp.path(),
            deadline: Instant::now() + Duration::from_secs(2),
            command: 0,
        };
        let args = [
            "/D".as_ref(),
            "/Q".as_ref(),
            "/C".as_ref(),
            "echo incomplete&ping -n 30 127.0.0.1 >nul".as_ref(),
        ];
        assert_eq!(probe.invoke(&args), Err(Rejection::Timeout));
    }

    #[cfg(unix)]
    fn scenario_observe(
        _: &mut Campaign<'_>,
        options: &Options,
        _: &str,
        _: bool,
    ) -> Result<Record> {
        let record = observe(options)?;
        if let Some(path) = &options.save {
            save(path, &serde_json::to_vec(&record)?)?;
        }
        Ok(record)
    }

    fn scenario_args(executable: &Path, options: &Options) -> Vec<OsString> {
        use crate::support::{cmd, path};
        let mut argv = vec![path(executable), "replica-progress".into()];
        for (flag, value) in [
            ("--binary", path(&options.binary)),
            ("--source", path(&options.source)),
            ("--stream", path(&options.stream)),
            ("--peer", path(&options.peer)),
            ("--credentials", path(&options.credentials)),
            ("--pin", path(&options.pin)),
            ("--timeout", options.timeout.to_string().into()),
        ] {
            argv.extend([flag.into(), value]);
        }
        if let Some(ship) = &options.ship {
            argv.extend(["--ship".into(), path(ship)]);
        } else {
            argv.extend(cmd(["--poll"]));
        }
        for (flag, file) in [
            ("--capture", &options.capture),
            ("--previous", &options.previous),
            ("--save", &options.save),
        ] {
            if let Some(file) = file {
                argv.extend([flag.into(), path(file)]);
            }
        }
        if let Some(samples) = options.samples {
            argv.extend(["--samples".into(), samples.to_string().into()]);
        }
        if let Some(interval) = options.interval_ms {
            argv.extend(["--interval-ms".into(), interval.to_string().into()]);
        }
        argv
    }

    #[cfg(windows)]
    fn scenario_observe(
        c: &mut Campaign<'_>,
        options: &Options,
        label: &str,
        failure: bool,
    ) -> Result<Record> {
        let argv = scenario_args(&c.output.join("xtask.exe"), options);
        let stdout = if failure {
            c.command_exit(argv, label, 1, 45.)?
        } else {
            c.command(argv, label, None, 45.)?
        };
        let record: Record = serde_json::from_value(delivery::json(stdout.as_bytes())?)?;
        ensure!(
            record.valid() && record.error.is_some() == failure,
            "invalid native observation"
        );
        Ok(record)
    }

    fn scenario_monitor(c: &mut Campaign<'_>, options: &Options) -> Result<Vec<serde_json::Value>> {
        admit(options)?;
        #[cfg(unix)]
        let executable = std::env::var_os("REPLICA_PROGRESS_XTASK").map(PathBuf::from);
        #[cfg(windows)]
        let executable = Some(c.output.join("xtask.exe"));
        let output = if let Some(executable) = executable {
            c.evidence["observer_sha256"] = serde_json::json!(crate::support::sha(&executable)?);
            c.command(
                scenario_args(&executable, options),
                "scheduled-polls",
                None,
                45.,
            )?
        } else {
            let mut output = Vec::new();
            monitor::run(options, options.samples.unwrap(), &mut output)?;
            String::from_utf8(output)?
        };
        output
            .lines()
            .map(|line| delivery::json(line.as_bytes()))
            .collect()
    }

    fn drop_responses<T>(
        remote: std::net::SocketAddr,
        send: impl FnOnce(std::net::SocketAddr) -> T,
    ) -> (T, u64) {
        use std::{
            net::{Shutdown, TcpListener, TcpStream},
            sync::atomic::{AtomicU64, Ordering},
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::scope(|scope| {
            let proxy = scope.spawn(|| {
                let deadline = Instant::now() + Duration::from_secs(8);
                let mut client = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("proxy accept: {error}"),
                    }
                };
                let mut server =
                    TcpStream::connect_timeout(&remote, Duration::from_secs(8)).unwrap();
                for socket in [&client, &server] {
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(8)))
                        .unwrap();
                    socket
                        .set_write_timeout(Some(Duration::from_secs(8)))
                        .unwrap();
                }
                let mut from_server = server.try_clone().unwrap();
                let mut to_client = client.try_clone().unwrap();
                let forwarded = AtomicU64::new(0);
                std::thread::scope(|relay| {
                    let down = relay.spawn(|| {
                        let mut discarded = 0;
                        let mut data = [0; 65_536];
                        while let Ok(n @ 1..) = from_server.read(&mut data) {
                            if forwarded.load(Ordering::Relaxed) > 8192 {
                                discarded += n as u64;
                            } else if to_client.write_all(&data[..n]).is_err() {
                                break;
                            }
                        }
                        discarded
                    });
                    let mut data = [0; 65_536];
                    while let Ok(n @ 1..) = client.read(&mut data) {
                        forwarded.fetch_add(n as u64, Ordering::Relaxed);
                        if server.write_all(&data[..n]).is_err() {
                            break;
                        }
                    }
                    let _ = client.shutdown(Shutdown::Both);
                    let _ = server.shutdown(Shutdown::Both);
                    down.join().unwrap()
                })
            });
            let result = send(address);
            (result, proxy.join().unwrap())
        })
    }

    #[test]
    #[ignore = "requires REPLICA_PROGRESS_BIN and REPLICA_PROGRESS_FIXTURE"]
    fn authenticated_observations_restarts_dropped_reply_and_stale_capture() {
        use crate::{
            snapshot_proof::{
                description_identity, finish_with_key_cleanup, receipt, selected_slot,
            },
            support::{Run, path, sha},
        };
        let binary =
            dunce::canonicalize(std::env::var_os("REPLICA_PROGRESS_BIN").unwrap()).unwrap();
        let fixture =
            dunce::canonicalize(std::env::var_os("REPLICA_PROGRESS_FIXTURE").unwrap()).unwrap();
        #[cfg(unix)]
        let tmp = Scratch::new(&std::env::temp_dir(), "native-observation").unwrap();
        #[cfg(unix)]
        let run = Run::new(tmp.path().to_owned());
        #[cfg(unix)]
        let output = std::env::var_os("REPLICA_PROGRESS_EVIDENCE")
            .map_or_else(|| tmp.path().join("report"), PathBuf::from);
        #[cfg(unix)]
        let mut c = Campaign::detached(&run, &output, "native authenticated observation").unwrap();
        #[cfg(unix)]
        let data = tmp.path().to_owned();
        #[cfg(windows)]
        let run = Run::new(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .to_owned(),
        );
        #[cfg(windows)]
        let mut c = Campaign::new(
            &run,
            Path::new(
                &std::env::var_os("REPLICA_PROGRESS_EVIDENCE")
                    .expect("fresh evidence output required"),
            ),
            "native authenticated observation",
        )
        .unwrap();
        #[cfg(windows)]
        let data = c.output.join("work");
        let follower = data.join("follower");
        let client = data.join("client");
        let server = data.join("server");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            #[cfg(windows)]
            let (binary, fixture) = {
                fs::create_dir(&data)?;
                c.restrict_directories(&[("work", &data)])?;
                c.env.retain(|name, _| {
                    !name.eq_ignore_ascii_case("TMP") && !name.eq_ignore_ascii_case("TEMP")
                });
                for name in ["TMP", "TEMP"] {
                    c.env.insert(name.into(), path(&data));
                }
                let cli = c.output.join("weekend-replicate.exe");
                let files = c.output.join("replica-progress-fixture.exe");
                let observer = c.output.join("xtask.exe");
                fs::copy(binary, &cli)?;
                fs::copy(fixture, &files)?;
                fs::copy(
                    std::env::var_os("REPLICA_PROGRESS_XTASK")
                        .context("native xtask executable required")?,
                    &observer,
                )?;
                c.evidence["observer_sha256"] = serde_json::json!(sha(&observer)?);
                (cli, files)
            };
            c.evidence["cli_sha256"] = serde_json::json!(sha(&binary)?);
            c.evidence["fixture_sha256"] = serde_json::json!(sha(&fixture)?);
            let mut options = options(&data);
            options.binary = binary;
            options.credentials = client.clone();
            options.pin = server.join("cert.der");
            options.timeout = 10.;
            fs::create_dir(&follower)?;
            c.command(
                vec![path(&fixture), "create".into(), path(&options.source)],
                "create",
                None,
                45.,
            )?;
            for (label, dir) in [("key-client", &client), ("key-server", &server)] {
                #[cfg(unix)]
                let keygen = &options.binary;
                #[cfg(windows)]
                let keygen = {
                    fs::create_dir(dir)?;
                    &fixture
                };
                c.command(
                    vec![path(keygen), "keygen".into(), path(dir)],
                    label,
                    None,
                    45.,
                )?;
            }
            let description = c.command(
                vec![
                    path(&options.binary),
                    "describe".into(),
                    path(&options.source),
                    STREAM.into(),
                ],
                "describe",
                None,
                45.,
            )?;
            let identity = description_identity(&description)?;
            c.command(
                vec![
                    path(&options.binary),
                    "follower-init".into(),
                    path(&follower),
                    STREAM.into(),
                    identity.clone().into(),
                ],
                "enroll",
                None,
                45.,
            )?;
            let serve = vec![
                path(&options.binary),
                "serve".into(),
                path(&follower),
                STREAM.into(),
                identity.clone().into(),
                "127.0.0.1:0".into(),
                path(&server),
                path(client.join("cert.der")),
                "--max-exchanges".into(),
                "20".into(),
            ];
            let limits = (Duration::from_secs(10), 150.);
            let export = data.join("first.export");
            let start = Instant::now();
            let mut monitor = monitor::Monitor::default();
            let mut number = 0;
            let mut measure = |record: Record| {
                number += 1;
                monitor.sample(number, start.elapsed(), Duration::ZERO, record)
            };
            c.loopback(serve.clone(), "initial-server", limits, false, |c, addr| {
                options.peer = addr.to_string();
                let empty = scenario_observe(c, &options, "empty", false)?;
                assert_eq!(empty.error, None);
                assert_eq!(empty.follower.state, State::Empty);
                let empty = measure(empty);
                assert_eq!(empty["waiting_for"]["revision"], 3);
                c.evidence["monitor_transitions"]["empty"] = empty;
                options.ship = Some(export.clone());
                options.poll = false;
                options.timeout = 3.;
                let (uncertain, discarded) = drop_responses(addr, |proxy| {
                    options.peer = proxy.to_string();
                    scenario_observe(c, &options, "lost-reply", true)
                });
                let uncertain = uncertain?;
                assert!(discarded > 0);
                assert!(uncertain.error.is_some());
                assert_eq!(uncertain.follower, Progress::empty(State::Unknown));
                c.evidence["discarded_tls_reply_bytes"] = serde_json::json!(discarded);
                Ok(())
            })?;
            options.ship = None;
            options.poll = true;
            let refused = scenario_observe(&mut c, &options, "refused-before-reopen", true)?;
            let refused = measure(refused);
            assert_eq!(refused["accepted"], false);
            assert!(refused["last_durable"].is_null());
            assert!(refused["caught_up"].is_null());
            c.evidence["monitor_transitions"]["refused_before_reopen"] = refused;
            let saved = data.join("prior.json");
            let first = c.loopback(serve, "restarted-server", limits, false, |c, addr| {
                options.peer = addr.to_string();
                options.ship = None;
                options.poll = true;
                options.timeout = 10.;
                options.capture = Some(export.clone());
                options.save = Some(saved.clone());
                let confirmed = scenario_observe(c, &options, "reopened-receipt", false)?;
                options.save = None;
                assert_eq!(confirmed.error, None);
                assert_eq!(confirmed.relation, Relation::ExactCapture);
                assert_eq!(confirmed.follower.revision, Some(3));
                let reopened = measure(confirmed.clone());
                assert_eq!(reopened["revision_gap"], 0);
                assert_eq!(reopened["caught_up"]["revision"], 3);
                c.evidence["monitor_transitions"]["reopened"] = reopened;
                #[cfg(unix)]
                assert_eq!(fs::metadata(&saved)?.permissions().mode() & 0o777, 0o600);
                options.previous = Some(saved.clone());
                let duplicate = scenario_observe(c, &options, "repeat-poll", false)?;
                assert_eq!(duplicate.error, None);
                assert_eq!(duplicate.prior.state, PriorState::Consistent);
                c.command(
                    vec![path(&fixture), "advance".into(), path(&options.source)],
                    "advance",
                    None,
                    45.,
                )?;
                let behind = measure(scenario_observe(c, &options, "behind", false)?);
                assert_eq!(behind["revision_gap"], 1);
                assert_eq!(behind["waiting_for"]["revision"], 4);
                c.evidence["monitor_transitions"]["behind"] = behind;
                let prior = options.previous.take();
                options.samples = Some(2);
                options.interval_ms = Some(100);
                let series = scenario_monitor(c, &options)?;
                assert_eq!(series.len(), 2);
                for (index, sample) in series.iter().enumerate() {
                    assert_eq!(sample["sample"], index + 1);
                    assert_eq!(sample["accepted"], true);
                    assert_eq!(sample["revision_gap"], 1);
                    assert_eq!(sample["last_durable"]["revision"], 3);
                    assert_eq!(sample["waiting_for"]["revision"], 4);
                    assert!(sample["caught_up"].is_null());
                }
                assert_eq!(series[1]["observation"]["prior"]["state"], "consistent");
                assert!(
                    series[1]["waiting_for"]["observed_for_ms"]
                        .as_u64()
                        .unwrap()
                        >= 100
                );
                c.evidence["scheduled_polls"] = serde_json::json!(series);
                options.samples = None;
                options.interval_ms = None;
                options.previous = prior;
                let second = data.join("second.export");
                options.ship = Some(second.clone());
                options.poll = false;
                options.capture = None;
                let newer = scenario_observe(c, &options, "ship-newer", false)?;
                assert_eq!(newer.error, None);
                assert_eq!(newer.follower.revision, Some(4));
                assert_eq!(newer.prior.state, PriorState::Superseded);
                options.ship = None;
                options.poll = true;
                options.capture = Some(export);
                let stale = scenario_observe(c, &options, "stale-capture", false)?;
                assert_eq!(stale.error, None);
                assert_eq!(stale.relation, Relation::NewerThanCapture);
                assert_ne!(stale.follower.digest, stale.capture.digest);
                let current = measure(stale);
                assert_eq!(current["revision_gap"], 0);
                assert_eq!(current["caught_up"]["revision"], 4);
                c.evidence["monitor_transitions"]["current"] = current;
                let changed = data.join("changed.export");
                fs::copy(second, &changed)?;
                OpenOptions::new()
                    .append(true)
                    .open(&changed)?
                    .write_all(&[0; 4096])?;
                options.capture = Some(changed);
                options.previous = None;
                assert_eq!(
                    scenario_observe(c, &options, "changed-export", true)?.error,
                    Some(Rejection::DigestConflict)
                );
                let mut foreign = confirmed.clone();
                foreign.enrollment.file_identity = Some("ff".repeat(16));
                let foreign_path = data.join("foreign.json");
                save(&foreign_path, &serde_json::to_vec(&foreign)?)?;
                options.capture = None;
                options.previous = Some(foreign_path);
                assert_eq!(
                    scenario_observe(c, &options, "foreign-prior", true)?.error,
                    Some(Rejection::IdentityMismatch)
                );
                Ok(confirmed)
            })?;
            options.previous = Some(saved);
            let unavailable = scenario_observe(&mut c, &options, "unavailable", true)?;
            assert_eq!(unavailable.prior.state, PriorState::HistoricalOnly);
            assert_eq!(unavailable.prior.observed_at, Some(first.observed_at));
            assert_eq!(unavailable.follower, Progress::empty(State::Unavailable));
            let unavailable = measure(unavailable);
            assert_eq!(unavailable["accepted"], false);
            assert!(unavailable["revision_gap"].is_null());
            assert_eq!(unavailable["last_durable"]["revision"], 4);
            c.evidence["monitor_transitions"]["refused_after_current"] = unavailable;
            let status = c.command(
                vec![
                    path(&options.binary),
                    "status".into(),
                    path(&follower),
                    STREAM.into(),
                    identity.into(),
                ],
                "final-status",
                None,
                45.,
            )?;
            assert_eq!(receipt(&status, STREAM)?.revision, 4);
            let file = follower.join(format!("replica-{}.wdb", selected_slot(&status)?));
            for (label, database) in [("verify-replica", file), ("verify-source", options.source)] {
                c.command(
                    vec![path(&fixture), "check".into(), path(database), "4".into()],
                    label,
                    None,
                    45.,
                )?;
            }
            Ok(())
        }))
        .unwrap_or_else(|_| {
            Err(anyhow::anyhow!(
                "native replica observation assertion failed"
            ))
        });
        let result =
            finish_with_key_cleanup(&mut c.evidence, &c.output, result, &[&client, &server]);
        c.finish(result).unwrap();
    }
}
