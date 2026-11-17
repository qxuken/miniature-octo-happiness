//! Test-only process-interruption helper; built with fault-injection.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod platform {
    use std::{
        io::{self, Write},
        path::Path,
        sync::{Arc, Mutex},
    };
    use weekend_db::{
        Database, Error, Limits, Result,
        storage::{ByteStore, FileStore},
    };
    struct Trace {
        armed: bool,
        events: Vec<&'static str>,
        cut: Option<(usize, bool)>,
    }
    fn park(message: std::fmt::Arguments<'_>) -> ! {
        println!("{message}");
        io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }
    struct CutStore {
        file: FileStore,
        trace: Arc<Mutex<Trace>>,
    }
    impl CutStore {
        fn before(&self, kind: &'static str) -> Option<usize> {
            let mut trace = self.trace.lock().unwrap();
            if !trace.armed {
                return None;
            }
            let index = trace.events.len();
            trace.events.push(kind);
            if trace.cut == Some((index, false)) {
                park(format_args!("CUT {index} {kind} before"));
            }
            Some(index)
        }
        fn after<T>(&self, index: Option<usize>, result: Result<T>) -> Result<T> {
            if let Some(index) = index
                && self.trace.lock().unwrap().cut == Some((index, true))
            {
                park(format_args!("CUT {index} after"));
            }
            result
        }
    }
    impl ByteStore for CutStore {
        fn lock(&mut self) -> Result<()> {
            self.file.lock()
        }
        fn unlock(&mut self) {
            self.file.unlock();
        }
        fn length(&mut self) -> Result<u64> {
            let index = self.before("length");
            let result = self.file.length();
            self.after(index, result)
        }
        fn read(&mut self, offset: u64, out: &mut [u8]) -> Result<usize> {
            let index = self.before("read");
            let result = self.file.read(offset, out);
            self.after(index, result)
        }
        fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<usize> {
            let index = self.before("write");
            let result = self.file.write(offset, bytes);
            self.after(index, result)
        }
        fn sync(&mut self) -> Result<()> {
            let index = self.before("sync");
            let result = self.file.sync();
            self.after(index, result)
        }
    }
    pub fn run() -> Result<()> {
        let args: Vec<_> = std::env::args().collect();
        let path = Path::new(&args[1]);
        let operation = &args[2];
        let phase = &args[3];
        #[cfg(not(windows))]
        if operation == "compact" {
            use weekend_db::storage::{CompactionStep as S, compact_file_with_hook};
            let events = Arc::new(Mutex::new(Vec::new()));
            let log = events.clone();
            let cut = if phase == "cut" {
                Some((args[4].parse::<usize>().unwrap(), args[5] == "after"))
            } else {
                None
            };
            compact_file_with_hook(path, Limits::default(), move |event| {
                let name = match event.step {
                    S::SourceWrite => "source-write",
                    S::SourceSync => "source-sync",
                    S::TargetWrite => "copy-write",
                    S::TargetSync => "copy-sync",
                    S::CopyMetadata => "metadata",
                    S::SyncMetadata => "metadata-sync",
                    S::CheckNames => "check-names",
                    S::Exchange => "exchange",
                    S::SyncDirectory => "directory-sync",
                    S::SyncReplacement => "replacement-sync",
                    S::RemoveOld => "remove-old",
                    S::SyncCleanupDirectory => "cleanup-directory",
                    S::SyncCleanupFile => "cleanup-sync",
                    _ => return Ok(()),
                };
                let mut events = log.lock().unwrap();
                if !event.after {
                    events.push(name);
                }
                let index = events.len() - 1;
                if cut == Some((index, event.after)) {
                    park(format_args!(
                        "CUT {index} {name} {}",
                        if event.after { "after" } else { "before" }
                    ));
                }
                Ok(())
            })?;
            let db = Database::open_file(path, Limits::default())?;
            let events = events.lock().unwrap();
            if phase == "trace" {
                println!(
                    "ACK {} {} {}",
                    events.len(),
                    db.snapshot().revision(),
                    events.join(",")
                );
            } else {
                park(format_args!(
                    "ACK {} {}",
                    events.len(),
                    db.snapshot().revision()
                ));
            }
            return Ok(());
        }
        if operation == "try" {
            match Database::open_file(path, Limits::default()) {
                Err(Error::Busy) => println!("BUSY"),
                Ok(_) => println!("OPEN"),
                Err(e) => return Err(e),
            }
            return Ok(());
        }
        if operation == "hold" {
            let _db = Database::open_file(path, Limits::default())?;
            park(format_args!("READY"));
        }
        let trace = Arc::new(Mutex::new(Trace {
            armed: operation == "recover" || operation == "create",
            events: Vec::new(),
            cut: if phase == "cut" {
                Some((args[4].parse().unwrap(), args[5] == "after"))
            } else {
                None
            },
        }));
        let file = if operation == "create" {
            FileStore::create(path)?
        } else {
            FileStore::open(path)?
        };
        let store = CutStore {
            file,
            trace: trace.clone(),
        };
        let db = if operation == "create" {
            Database::create_on(store, *b"file-worker-0001", Limits::default())?
        } else {
            Database::open_on(store, Limits::default())?
        };
        trace.lock().unwrap().armed = true;
        match operation.as_str() {
            "data" => {
                let mut tx = db.begin();
                let v = db.integer(3)?;
                tx.put(db.collection_id(1), b"a", &v)?;
                tx.put(db.collection_id(1), b"b", &v)?;
                tx.commit()?;
            }
            "retain" => {
                db.retain_snapshot(&db.snapshot())?;
            }
            "release" => db.release_retained(1)?,
            "recover" | "create" => {}
            _ => return Err(Error::Invalid),
        }
        let trace = trace.lock().unwrap();
        let revision = db.snapshot().revision();
        if phase == "trace" {
            println!(
                "ACK {} {revision} {}",
                trace.events.len(),
                trace.events.join(",")
            );
        } else {
            park(format_args!("ACK {} {revision}", trace.events.len()));
        }
        Ok(())
    }
}
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn main() -> weekend_db::Result<()> {
    platform::run()
}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn main() {
    panic!("file-worker requires macOS, Linux, or Windows");
}
