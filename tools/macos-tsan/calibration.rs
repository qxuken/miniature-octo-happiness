use std::{
    ffi::c_int,
    fs, io,
    sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
    thread,
    time::Duration,
};

unsafe extern "C" {
    fn __error() -> *mut c_int;
}

static mut SHARED: c_int = 0;
static COMPLETED: AtomicUsize = AtomicUsize::new(0);

fn errno_reuse() {
    const ROUNDS: usize = 256;
    for _ in 0..ROUNDS {
        drop(thread::spawn(|| {
            for _ in 0..32 {
                let _ = fs::read_dir(std::env::temp_dir()).unwrap().next();
            }
            COMPLETED.fetch_add(1, Relaxed);
        }));
        thread::sleep(Duration::from_millis(2));
        drop(thread::spawn(|| {
            for _ in 0..32 {
                std::hint::black_box(io::Error::last_os_error());
            }
            COMPLETED.fetch_add(1, Relaxed);
        }));
    }
    while COMPLETED.load(Relaxed) < 2 * ROUNDS {
        thread::sleep(Duration::from_millis(2));
    }
}

fn deliberate_race(address: usize) {
    let barrier = Barrier::new(2);
    thread::scope(|scope| {
        for value in [1, 2] {
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                // Deliberate race in this disposable diagnostic process.
                unsafe { std::ptr::write_volatile(address as *mut c_int, value) };
            });
        }
    });
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("errno-reuse") => errno_reuse(),
        Some("race") => deliberate_race(&raw mut SHARED as usize),
        Some("errno-race") => deliberate_race(unsafe { __error() } as usize),
        _ => panic!("unknown ThreadSanitizer calibration"),
    }
}
