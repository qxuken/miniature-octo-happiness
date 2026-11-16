#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod native;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn main() -> std::process::ExitCode {
    diagnostics::entry(diagnostics::App::Snapshot, native::main)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn main() {
    eprintln!("weekend-replicate requires native file storage");
    std::process::exit(1);
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[path = "../../support/service_diagnostics.rs"]
mod diagnostics;
