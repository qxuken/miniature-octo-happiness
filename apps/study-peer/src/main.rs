#[cfg(any(target_os = "macos", target_os = "linux", windows))]
mod native;

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn main() -> std::process::ExitCode {
    diagnostics::entry(diagnostics::App::Study, native::main)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn main() -> std::process::ExitCode {
    eprintln!("study-peer requires native file storage");
    std::process::ExitCode::FAILURE
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[path = "../../support/service_diagnostics.rs"]
mod diagnostics;
