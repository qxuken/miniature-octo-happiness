//! Native Windows child-tree ownership. A child stays suspended until its job
//! owns it; every outcome terminates the job and verifies zero active processes.
use crate::trace_process::{Report, Request};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_NO_MORE_FILES, GetLastError, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_SUSPENDED, GetCurrentProcess, IsWow64Process2, OpenThread, ResumeThread,
            THREAD_SUSPEND_RESUME,
        },
    },
};
pub(crate) const JOB_TERMINATION_EXIT: u32 = 0xEE;

struct Handle(HANDLE);
impl Handle {
    fn new(value: HANDLE) -> Result<Self> {
        ensure!(
            !value.is_null() && value != INVALID_HANDLE_VALUE,
            "Windows handle: {}",
            std::io::Error::last_os_error()
        );
        Ok(Self(value))
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: this wrapper uniquely owns a valid handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Job(Handle);
impl Job {
    fn own_and_resume(child: &Child) -> Result<Self> {
        // SAFETY: null attributes/name create an unnamed noninheritable job.
        let job = Self(Handle::new(unsafe {
            CreateJobObjectW(ptr::null(), ptr::null())
        })?);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: initialized, correctly sized structure and live handles. The
        // std child handle is borrowed; the suspended process cannot spawn yet.
        ensure!(
            unsafe {
                SetInformationJobObject(
                    job.0.0,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32,
                )
            } != 0,
            "job limits: {}",
            std::io::Error::last_os_error()
        );
        ensure!(
            unsafe { AssignProcessToJobObject(job.0.0, child.as_raw_handle()) } != 0,
            "job assignment: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: take an owned snapshot and enumerate initialized thread entries.
        let snapshot = Handle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })?;
        let mut entry = THREADENTRY32 {
            dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut found = vec![];
        let mut valid = unsafe { Thread32First(snapshot.0, &mut entry) };
        while valid != 0 {
            if entry.th32OwnerProcessID == child.id() {
                found.push(entry.th32ThreadID);
            }
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            valid = unsafe { Thread32Next(snapshot.0, &mut entry) };
        }
        ensure!(
            unsafe { GetLastError() } == ERROR_NO_MORE_FILES,
            "incomplete primary thread enumeration"
        );
        ensure!(found.len() == 1, "one suspended primary thread required");
        // SAFETY: the still-unreaped, suspended owned child keeps its thread ID
        // live. The temporary handle grants only suspend/resume permission.
        let primary = Handle::new(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, found[0]) })?;
        ensure!(
            unsafe { ResumeThread(primary.0) } == 1,
            "cannot resume owned primary thread"
        );
        Ok(job)
    }
    fn retire(&self) -> Result<()> {
        // SAFETY: the live job owns all descendants; termination is unconditional.
        ensure!(
            unsafe { TerminateJobObject(self.0.0, JOB_TERMINATION_EXIT) } != 0,
            "terminate job: {}",
            std::io::Error::last_os_error()
        );
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            // SAFETY: correctly sized writable accounting structure, live job.
            ensure!(
                unsafe {
                    QueryInformationJobObject(
                        self.0.0,
                        JobObjectBasicAccountingInformation,
                        (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                        std::mem::size_of_val(&info) as u32,
                        ptr::null_mut(),
                    )
                } != 0,
                "query job: {}",
                std::io::Error::last_os_error()
            );
            if info.ActiveProcesses == 0 {
                return Ok(());
            }
            ensure!(Instant::now() < end, "job children remain active");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: best-effort fallback, then KILL_ON_JOB_CLOSE closes the job.
        unsafe {
            TerminateJobObject(self.0.0, JOB_TERMINATION_EXIT);
        }
    }
}
fn drain(
    mut pipe: impl Read + Send + 'static,
    mut file: impl Write + Send + 'static,
    maximum: u64,
    overflow: Arc<AtomicBool>,
    failures: Arc<Mutex<Vec<String>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let result = (|| -> Result<()> {
            let mut bytes = [0; 65536];
            let mut total = 0;
            loop {
                let n = pipe.read(&mut bytes)?;
                if n == 0 {
                    break;
                }
                let allowed = (maximum - total).min(n as u64) as usize;
                file.write_all(&bytes[..allowed])?;
                total += allowed as u64;
                if allowed != n {
                    overflow.store(true, Ordering::Relaxed);
                    break;
                }
            }
            file.flush()?;
            Ok(())
        })();
        if let Err(e) = result {
            failures
                .lock()
                .unwrap()
                .push(format!("pipe capture: {e:#}"));
            overflow.store(true, Ordering::Relaxed);
        }
    })
}
pub fn run(request: &Request<'_>, stop: Option<&AtomicBool>) -> Result<Report> {
    let stdout = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(request.stdout)?;
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(request.stderr)?;
    owned(request, stdout, stderr, stop)
}
fn owned(
    request: &Request<'_>,
    stdout: impl Write + Send + 'static,
    stderr: impl Write + Send + 'static,
    stop: Option<&AtomicBool>,
) -> Result<Report> {
    let started = Instant::now();
    let mut child = Command::new(&request.argv[0])
        .args(&request.argv[1..])
        .current_dir(request.cwd)
        .env_clear()
        .envs(request.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_SUSPENDED)
        .spawn()
        .context("spawn suspended Windows trace child")?;
    let mut report = Report {
        pid: Some(child.id()),
        ..Default::default()
    };
    let mut job = None;
    let mut readers = vec![];
    let overflow = Arc::new(AtomicBool::new(false));
    let failures = Arc::new(Mutex::new(vec![]));
    let execution = (|| -> Result<()> {
        job = Some(Job::own_and_resume(&child)?);
        readers.push(drain(
            child.stdout.take().unwrap(),
            stdout,
            request.maximum,
            overflow.clone(),
            failures.clone(),
        ));
        readers.push(drain(
            child.stderr.take().unwrap(),
            stderr,
            request.maximum,
            overflow.clone(),
            failures.clone(),
        ));
        loop {
            ensure!(
                !overflow.load(Ordering::Relaxed),
                "command output limit/read failure"
            );
            if let Some(status) = child.try_wait()? {
                report.exit = status.code().map(|n| i64::from(n as u32));
                break;
            }
            ensure!(
                started.elapsed().as_secs_f64() < request.timeout,
                "command deadline"
            );
            if stop.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                report.stopped = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    })();
    if let Err(e) = execution {
        failures.lock().unwrap().push(format!("{e:#}"));
    }
    let mut clean = false;
    if let Some(job) = &job {
        match job.retire() {
            Ok(()) => clean = true,
            Err(e) => failures
                .lock()
                .unwrap()
                .push(format!("job retirement: {e:#}")),
        }
    }
    // Also covers a failure before job assignment while the child is suspended.
    let _ = child.kill();
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                report.exit = status.code().map(|n| i64::from(n as u32));
                break;
            }
            Ok(None) if Instant::now() < end => thread::sleep(Duration::from_millis(10)),
            other => {
                failures
                    .lock()
                    .unwrap()
                    .push(format!("child retirement failed: {other:?}"));
                clean = false;
                break;
            }
        }
    }
    let end = Instant::now() + Duration::from_secs(5);
    while readers.iter().any(|r| !r.is_finished()) && Instant::now() < end {
        thread::sleep(Duration::from_millis(10));
    }
    for reader in readers {
        if reader.is_finished() {
            if reader.join().is_err() {
                failures
                    .lock()
                    .unwrap()
                    .push("capture thread panicked".into());
                clean = false;
            }
        } else {
            failures
                .lock()
                .unwrap()
                .push("command pipes remain open".into());
            clean = false;
        }
    }
    if overflow.load(Ordering::Relaxed) {
        failures
            .lock()
            .unwrap()
            .push("command output limit/read failure".into());
    }
    report.cleanup_verified = clean;
    report.seconds = started.elapsed().as_secs_f64();
    let errors = failures.lock().unwrap();
    if !errors.is_empty() {
        report.failure = Some(errors.join("; "));
    }
    Ok(report)
}
struct Memory(Arc<Mutex<Vec<u8>>>);
impl Write for Memory {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Environment discovery may contain credentials: both streams are bounded,
/// owned by the suspended-child job, and kept entirely off disk and out of errors.
pub fn capture_private(
    argv: &[std::ffi::OsString],
    cwd: &Path,
    env: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    timeout: f64,
) -> Result<Vec<u8>> {
    let bytes = Arc::new(Mutex::new(vec![]));
    let request = Request {
        argv,
        cwd,
        env,
        stdout: Path::new("unused-private-output"),
        stderr: Path::new("unused-private-error"),
        timeout,
        maximum: 4 * 1024 * 1024,
    };
    let report = owned(&request, Memory(bytes.clone()), std::io::sink(), None)?;
    ensure!(
        report.qualifies(false),
        "private toolchain process failed: {report:?}"
    );
    let mut data = bytes.lock().unwrap();
    Ok(std::mem::take(&mut *data))
}
pub fn volume(directory: &Path) -> Result<Value> {
    let name: Vec<u16> = directory.as_os_str().encode_wide().chain([0]).collect();
    let mut root = [0u16; 32768];
    let mut filesystem = [0u16; 64];
    let (mut serial, mut component, mut flags, mut process, mut native) = (0, 0, 0, 0, 0);
    // SAFETY: initialized native outputs, valid terminated UTF-16 input/buffers.
    unsafe {
        ensure!(
            IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) != 0,
            "native architecture query failed"
        );
        ensure!(
            process == 0 && native == 0x8664,
            "native x64 Windows required; emulation is separate"
        );
        ensure!(
            GetVolumePathNameW(name.as_ptr(), root.as_mut_ptr(), root.len() as u32) != 0,
            "data volume lookup failed"
        );
        ensure!(
            GetVolumeInformationW(
                root.as_ptr(),
                ptr::null_mut(),
                0,
                &mut serial,
                &mut component,
                &mut flags,
                filesystem.as_mut_ptr(),
                filesystem.len() as u32
            ) != 0,
            "data filesystem query failed"
        );
    }
    let root_name = String::from_utf16(
        &root[..root
            .iter()
            .position(|n| *n == 0)
            .context("volume name terminator")?],
    )?;
    let filesystem = String::from_utf16(
        &filesystem[..filesystem
            .iter()
            .position(|n| *n == 0)
            .context("filesystem terminator")?],
    )?;
    // SAFETY: root remains a valid terminated volume name.
    let drive = unsafe { GetDriveTypeW(root.as_ptr()) };
    ensure!(
        filesystem.eq_ignore_ascii_case("NTFS") && drive == 3,
        "local fixed NTFS data volume required"
    );
    Ok(
        json!({"root":root_name,"filesystem":filesystem,"drive_type":drive,"serial":serial,"flags":flags,"process_machine":process,"native_machine":native}),
    )
}

#[cfg(test)]
mod tests {
    use crate::support::{Run, Scratch, cmd};
    #[test]
    fn private_capture_owns_children_without_persisting_environment_output() {
        let t = Scratch::new(&std::env::temp_dir(), "windows-private-capture").unwrap();
        let mut run = Run::new(t.path().to_owned());
        let secret = "private-fixture-environment-value";
        run.env
            .insert("XTASK_PRIVATE_FIXTURE_VALUE".into(), secret.into());
        let output = run
            .capture_private(&cmd([
                "cmd.exe",
                "/D",
                "/Q",
                "/C",
                "echo %XTASK_PRIVATE_FIXTURE_VALUE%",
            ]))
            .unwrap();
        assert_eq!(String::from_utf8(output).unwrap().trim(), secret);
        let failure = run
            .capture_private(&cmd([
                "cmd.exe",
                "/D",
                "/Q",
                "/C",
                "echo %XTASK_PRIVATE_FIXTURE_VALUE% & exit /b 2",
            ]))
            .unwrap_err();
        assert!(!format!("{failure:#}").contains(secret));
        assert!(
            !serde_json::to_string(&run.commands)
                .unwrap()
                .contains(secret)
        );
        assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
        run.private_deadline = Some(std::time::Instant::now());
        assert!(
            run.capture_private(&cmd(["cmd.exe", "/D", "/Q", "/C", "exit /b 0"]))
                .unwrap_err()
                .to_string()
                .contains("deadline")
        );
    }
}
