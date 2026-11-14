#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod native;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn main() -> std::process::ExitCode {
    match native::main() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn main() {
    eprintln!("weekend-replicate requires native file storage");
    std::process::exit(1);
}
