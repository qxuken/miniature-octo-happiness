#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn run(args: &[std::ffi::OsString]) -> weekend_db::Result<()> {
    use std::io::Read;
    use storage::ByteStore;
    use weekend_db::{Limits, storage};
    match args {
        [command, source, destination] if command == "salvage" => {
            let report = storage::salvage_file(source, destination, Limits::default())?;
            println!(
                "salvaged revision {} data {} retained {} from slot {}",
                report.revision, report.data_revision, report.retained_roots, report.selected_slot
            );
            println!("verified slots {:?}", report.verified_slot_revisions);
            println!("witnesses ignored, acknowledged data loss possible");
            Ok(())
        }
        [command, path] if command == "check" => {
            let info = storage::inspect_file(path, Limits::default())?;
            println!(
                "format {} revision {} data {} witnessed {}",
                info.format_version, info.revision, info.data_revision, info.witnessed
            );
            println!("identity {:02x?}", info.identity);
            println!(
                "physical bytes {} committed bytes {}",
                info.physical_bytes, info.committed_bytes
            );
            for root in info.retained {
                println!(
                    "retained {} revision {} data {}",
                    root.id, root.revision, root.data_revision
                );
            }
            Ok(())
        }
        [command, source, destination] if command == "backup" || command == "restore" => {
            let stats = if command == "backup" {
                storage::backup_file(source, destination, Limits::default())?
            } else {
                storage::restore_file(source, destination, Limits::default())?
            };
            println!(
                "verified copy {} bytes from {} bytes",
                stats.after_bytes, stats.before_bytes
            );
            Ok(())
        }
        [command, source, destination] if command == "export" => {
            let bytes = storage::export_logical_file(source, Limits::default())?;
            let mut file = storage::FileStore::create(destination)?;
            file.lock()?;
            let mut offset = 0;
            while offset < bytes.len() {
                let written = file.write(offset as u64, &bytes[offset..])?;
                if written == 0 || written > bytes.len() - offset {
                    return Err(weekend_db::Error::Io);
                }
                offset += written;
            }
            file.sync()?;
            file.unlock();
            println!("exported logical format 1");
            Ok(())
        }
        [command, source, destination] if command == "import" => {
            let file = std::fs::File::open(source).map_err(|_| weekend_db::Error::Io)?;
            let mut bytes = Vec::new();
            file.take(storage::MAX_IMAGE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| weekend_db::Error::Io)?;
            storage::import_logical_file(&bytes, destination, Limits::default())?;
            println!("imported and verified logical format 1");
            Ok(())
        }
        _ => Err(weekend_db::Error::Invalid),
    }
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    let result = run(&args);
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let result: weekend_db::Result<()> = {
        let _ = args;
        Err(weekend_db::Error::UnsupportedPlatform)
    };
    if let Err(error) = result {
        eprintln!("{error}");
        eprintln!(
            "usage: weekend-admin check FILE | backup SOURCE NEW_FILE | restore BACKUP NEW_FILE | salvage SOURCE NEW_FILE | export SOURCE NEW_LOGICAL | import LOGICAL NEW_FILE"
        );
        std::process::exit(1);
    }
}
