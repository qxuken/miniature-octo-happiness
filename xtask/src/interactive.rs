//! Bounded line-oriented exchange with an owned, supervised process group.
use crate::supervise::{self, Report, Request};
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::{self, PipeWriter, Write},
    os::{fd::AsRawFd, unix::fs::FileExt},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_LINE: usize = 4096;
const IO_WAIT: Duration = Duration::from_secs(15);
const CLOSE_WAIT: Duration = Duration::from_secs(10);
const TICK: Duration = Duration::from_millis(10);

pub struct Session {
    input: PipeWriter,
    stdout: PathBuf,
    offset: u64,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<Report>>>,
}
impl Session {
    pub fn start(request: Request) -> Result<Self> {
        let (read, write) = io::pipe()?;
        supervise::nonblocking(write.as_raw_fd())?;
        let stop = Arc::new(AtomicBool::new(false));
        let child_stop = Arc::clone(&stop);
        let stdout = request.stdout.clone();
        let worker = thread::Builder::new()
            .name("interactive-supervisor".into())
            .spawn(move || supervise::run_interactive(&request, &child_stop, read))?;
        Ok(Self {
            input: write,
            stdout,
            offset: 0,
            stop,
            worker: Some(worker),
        })
    }

    pub fn line(&mut self) -> Result<String> {
        self.line_until(Instant::now() + IO_WAIT)
    }

    fn line_until(&mut self, deadline: Instant) -> Result<String> {
        let mut buffer = [0; MAX_LINE + 1];
        loop {
            // Check completion before inspecting the log. A completed
            // supervisor has drained the final bytes and closed its files.
            let ended = self.worker.as_ref().is_none_or(JoinHandle::is_finished);
            let read = match File::open(&self.stdout) {
                Ok(file) => file.read_at(&mut buffer, self.offset)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
                Err(error) => return Err(error.into()),
            };
            if let Some(newline) = buffer[..read].iter().position(|b| *b == b'\n') {
                ensure!(newline < MAX_LINE, "interactive reply exceeds line bound");
                self.offset += (newline + 1) as u64;
                let row = std::str::from_utf8(&buffer[..newline])?;
                return Ok(row.trim_end_matches('\r').to_owned());
            }
            ensure!(read < MAX_LINE, "interactive reply exceeds line bound");
            ensure!(!ended, "interactive process exited before replying");
            ensure!(Instant::now() < deadline, "interactive reply deadline");
            thread::sleep(TICK);
        }
    }

    fn send_until(&mut self, line: &str, deadline: Instant) -> Result<()> {
        ensure!(
            !line.is_empty() && line.len() < MAX_LINE && !line.contains(['\r', '\n']),
            "invalid interactive command"
        );
        let bytes = format!("{line}\n");
        let mut rest = bytes.as_bytes();
        while !rest.is_empty() {
            ensure!(Instant::now() < deadline, "interactive input deadline");
            match self.input.write(rest) {
                Ok(0) => anyhow::bail!("closed interactive input"),
                Ok(count) => rest = &rest[count..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(TICK),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub fn command(&mut self, line: &str) -> Result<String> {
        let deadline = Instant::now() + IO_WAIT;
        self.send_until(line, deadline)?;
        self.line_until(deadline)
    }

    pub fn send(&mut self, line: &str) -> Result<()> {
        self.send_until(line, Instant::now() + IO_WAIT)
    }

    pub fn close(mut self, kill: bool) -> Result<Report> {
        let deadline = Instant::now() + CLOSE_WAIT;
        if kill {
            self.stop.store(true, Ordering::Relaxed);
        }
        let worker = self
            .worker
            .take()
            .context("interactive supervisor missing")?;
        while !worker.is_finished() && Instant::now() < deadline {
            thread::sleep(TICK);
        }
        let timed_out = !worker.is_finished();
        if timed_out {
            self.stop.store(true, Ordering::Relaxed);
        }
        let report = worker
            .join()
            .map_err(|_| anyhow::anyhow!("interactive supervisor panicked"))??;
        ensure!(!timed_out, "interactive process exceeded exit deadline");
        ensure!(
            report.cleanup_verified && report.cleanup_failure.is_none(),
            "interactive ownership cleanup failed"
        );
        Ok(report)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.stop.store(true, Ordering::Relaxed);
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::Scratch;
    use std::path::Path;

    fn request(root: &Path, script: &str) -> Request {
        Request {
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            cwd: Some(root.to_owned()),
            env: None,
            stdout: root.join("stdout"),
            stderr: root.join("stderr"),
            report: Some(root.join("report.json")),
            timeout_seconds: 5.,
            maximum_bytes: 64 * 1024,
            exclusive_limit: false,
            file_limit: None,
            journal: None,
            sample_rss: false,
        }
    }

    #[test]
    fn one_command_consumes_one_bounded_reply_and_graceful_exit_is_reaped() {
        let scratch = Scratch::new(&std::env::temp_dir(), "interactive").unwrap();
        let request = request(
            scratch.path(),
            "printf 'ready\\n'; while IFS= read -r line; do case \"$line\" in quit) exit 0;; *) printf 'echo %s\\n' \"$line\";; esac; done",
        );
        let mut session = Session::start(request).unwrap();
        assert_eq!(session.line().unwrap(), "ready");
        assert!(session.send("extra\ncommand").is_err());
        assert_eq!(session.command("one").unwrap(), "echo one");
        assert_eq!(session.command("two").unwrap(), "echo two");
        session.send("quit").unwrap();
        assert!(session.close(false).unwrap().success());
    }

    #[test]
    fn oversized_reply_rejects_and_drop_retires_the_live_group() {
        let scratch = Scratch::new(&std::env::temp_dir(), "interactive").unwrap();
        let request = request(scratch.path(), "head -c 4096 /dev/zero; exec sleep 30");
        let mut session = Session::start(request).unwrap();
        assert!(
            session
                .line()
                .unwrap_err()
                .to_string()
                .contains("line bound")
        );
        drop(session);
        let report = crate::delivery::read_json(&scratch.path().join("report.json")).unwrap();
        assert_eq!(report["stopped"], true);
        assert_eq!(report["cleanup_verified"], true);
    }
}
