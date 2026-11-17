//! Small process/canary fixture shared by the two service diagnostic targets.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub const PATH_CANARY: &str = "F06_PRIVATE_PATH_CANARY";
pub const CREDENTIAL_CANARY: &str = "F06_PRIVATE_CREDENTIAL_CANARY";
pub const RECORD_CANARY: &str = "F06_PRIVATE_RECORD_CANARY";
pub const ID: &str = "11111111111111111111111111111111";
pub const BUDGET: Duration = Duration::from_secs(10);
pub struct Root(pub PathBuf, &'static str);
impl Root {
    pub fn new(app: &'static str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "{app}-diagnostics-{PATH_CANARY}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        }
        #[cfg(windows)]
        fs::create_dir(&path).unwrap();
        Self(path, app)
    }
    pub fn credentials(&self, name: &str) -> PathBuf {
        let directory = self.0.join(name);
        fs::create_dir(&directory).unwrap();
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec![self.1.into()]).unwrap();
        fs::write(directory.join("cert.der"), cert.der()).unwrap();
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(directory.join("key.der"))
            .unwrap()
            .write_all(&signing_key.serialize_der())
            .unwrap();
        directory
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub struct Process {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}
impl Process {
    pub fn start(root: &Path, label: &str, mut command: Command) -> Self {
        let stdout = root.join(format!("{label}.stdout"));
        let stderr = root.join(format!("{label}.stderr"));
        let child = command
            .stdin(Stdio::piped())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap())
            .spawn()
            .unwrap();
        Self {
            child,
            stdout,
            stderr,
        }
    }
    pub fn logs(&self) -> (String, String) {
        for file in [&self.stdout, &self.stderr] {
            assert!(
                fs::metadata(file).unwrap().len() <= 16 * 1024,
                "diagnostic output exceeded bound"
            );
        }
        (
            fs::read_to_string(&self.stdout).unwrap(),
            fs::read_to_string(&self.stderr).unwrap(),
        )
    }
    pub fn until(&mut self, ready: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + BUDGET;
        loop {
            let (stdout, _) = self.logs();
            if ready(&stdout) {
                return stdout;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "service exited before observation"
            );
            assert!(Instant::now() < deadline, "diagnostic observation deadline");
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[allow(dead_code)] // Only the interactive Study service uses stdin commands.
    pub fn input(&mut self, text: &str) {
        self.child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        self.child.stdin.as_mut().unwrap().flush().unwrap();
    }
    pub fn finish(&mut self, success: bool) -> (String, String) {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + BUDGET;
        loop {
            self.logs();
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(status.success(), success, "unexpected process status");
                return self.logs();
            }
            assert!(Instant::now() < deadline, "service retirement deadline");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub fn safe(logs: &(String, String)) {
    for output in [&logs.0, &logs.1] {
        for canary in [PATH_CANARY, CREDENTIAL_CANARY, RECORD_CANARY] {
            assert!(
                !output.contains(canary),
                "private canary appeared in operational output"
            );
        }
    }
}
