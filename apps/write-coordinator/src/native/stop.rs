use std::{io, net::TcpListener, sync::atomic::AtomicBool};

#[path = "../../../support/listener_ready.rs"]
mod listener_ready;

const WAIT_MS: i32 = 50;

pub(super) fn wait_for_connection(listener: &TcpListener) -> io::Result<bool> {
    listener_ready::wait_for_connection(listener, WAIT_MS)
}

#[cfg(unix)]
pub(super) struct Stop {
    requested: std::sync::Arc<AtomicBool>,
    handlers: Vec<signal_hook::SigId>,
}

#[cfg(unix)]
impl Stop {
    pub(super) fn register() -> io::Result<Self> {
        let mut stop = Self {
            requested: std::sync::Arc::new(AtomicBool::new(false)),
            handlers: Vec::new(),
        };
        for signal in [libc::SIGINT, libc::SIGTERM] {
            stop.handlers
                .push(signal_hook::flag::register(signal, stop.requested.clone())?);
        }
        Ok(stop)
    }

    pub(super) fn requested(&self) -> &AtomicBool {
        &self.requested
    }
}

#[cfg(unix)]
impl Drop for Stop {
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            signal_hook::low_level::unregister(handler);
        }
    }
}

#[cfg(windows)]
use std::sync::atomic::Ordering::Relaxed;
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, GetConsoleCP, SetConsoleCtrlHandler,
};

#[cfg(windows)]
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
pub(super) struct Stop {
    registered: bool,
}

#[cfg(windows)]
unsafe extern "system" fn console_stop(event: u32) -> i32 {
    match event {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => {
            STOP_REQUESTED.store(true, Relaxed);
            1
        }
        _ => 0,
    }
}

#[cfg(windows)]
impl Stop {
    pub(super) fn register() -> io::Result<Self> {
        STOP_REQUESTED.store(false, Relaxed);
        // SAFETY: queries the process's own console and accepts no pointers.
        let registered = unsafe { GetConsoleCP() } != 0;
        // SAFETY: the callback has the required ABI and only uses a static atomic.
        if registered && unsafe { SetConsoleCtrlHandler(Some(console_stop), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { registered })
    }

    pub(super) fn requested(&self) -> &AtomicBool {
        &STOP_REQUESTED
    }
}

#[cfg(windows)]
impl Drop for Stop {
    fn drop(&mut self) {
        if self.registered {
            // SAFETY: removes the same static callback registered by this instance.
            unsafe { SetConsoleCtrlHandler(Some(console_stop), 0) };
        }
    }
}
