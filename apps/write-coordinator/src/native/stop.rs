use std::{io, net::TcpListener, sync::atomic::AtomicBool};

const WAIT_MS: i32 = 50;

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

#[cfg(unix)]
pub(super) fn wait_for_connection(listener: &TcpListener) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    let mut descriptor = libc::pollfd {
        fd: listener.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: the listener stays open and descriptor holds one writable pollfd.
    let ready = unsafe { libc::poll(&mut descriptor, 1, WAIT_MS) };
    if ready < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ready != 0)
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

#[cfg(windows)]
pub(super) fn wait_for_connection(listener: &TcpListener) -> io::Result<bool> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        POLLRDNORM, SOCKET_ERROR, WSAGetLastError, WSAPOLLFD, WSAPoll,
    };
    let mut descriptor = WSAPOLLFD {
        fd: listener.as_raw_socket() as usize,
        events: POLLRDNORM,
        revents: 0,
    };
    // SAFETY: the socket stays open and descriptor holds one writable WSAPOLLFD.
    let ready = unsafe { WSAPoll(&mut descriptor, 1, WAIT_MS) };
    if ready == SOCKET_ERROR {
        // SAFETY: called on the same thread that made the failed Winsock call.
        Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }))
    } else {
        Ok(ready != 0)
    }
}
