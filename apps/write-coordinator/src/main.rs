//! One local process owns the file and handles conditional writes from clients.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod native;

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn main() -> std::process::ExitCode {
    match native::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn main() -> std::process::ExitCode {
    eprintln!("write-coordinator requires native file storage");
    std::process::ExitCode::FAILURE
}
