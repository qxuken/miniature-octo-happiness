#[cfg(any(target_os = "macos", target_os = "linux", windows))]
#[path = "live_month/native.rs"]
mod native;

fn main() {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    if let Err(error) = native::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        eprintln!("live_month requires native file storage");
        std::process::exit(1);
    }
}
