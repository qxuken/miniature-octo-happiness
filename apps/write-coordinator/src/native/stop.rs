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
        Self::register_with(signal_hook::flag::register)
    }

    fn register_with(
        mut register: impl FnMut(i32, std::sync::Arc<AtomicBool>) -> io::Result<signal_hook::SigId>,
    ) -> io::Result<Self> {
        let mut stop = Self {
            requested: std::sync::Arc::new(AtomicBool::new(false)),
            handlers: Vec::new(),
        };
        for signal in [libc::SIGINT, libc::SIGTERM] {
            stop.handlers
                .push(register(signal, stop.requested.clone())?);
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

#[cfg(any(windows, test))]
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
    request_console_stop(event, [CTRL_C_EVENT, CTRL_BREAK_EVENT], &STOP_REQUESTED)
}

#[cfg(any(windows, test))]
fn request_console_stop(event: u32, stop_events: [u32; 2], requested: &AtomicBool) -> i32 {
    if stop_events.contains(&event) {
        requested.store(true, Relaxed);
        1
    } else {
        0
    }
}

#[cfg(windows)]
impl Stop {
    pub(super) fn register() -> io::Result<Self> {
        STOP_REQUESTED.store(false, Relaxed);
        // SAFETY: queries the process's own console and accepts no pointers.
        let registered = unsafe { GetConsoleCP() } != 0;
        register_console(
            registered,
            || {
                // SAFETY: the callback has the required ABI and only uses a static atomic.
                unsafe { SetConsoleCtrlHandler(Some(console_stop), 1) }
            },
            io::Error::last_os_error,
        )
        .map(|()| Self { registered })
    }

    pub(super) fn requested(&self) -> &AtomicBool {
        &STOP_REQUESTED
    }
}

#[cfg(windows)]
impl Drop for Stop {
    fn drop(&mut self) {
        unregister_console(self.registered, || {
            // SAFETY: removes the same static callback registered by this instance.
            unsafe { SetConsoleCtrlHandler(Some(console_stop), 0) };
        });
    }
}

#[cfg(any(windows, test))]
fn register_console(
    attached: bool,
    register: impl FnOnce() -> i32,
    last_error: impl FnOnce() -> io::Error,
) -> io::Result<()> {
    if attached && register() == 0 {
        return Err(last_error());
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn unregister_console(registered: bool, unregister: impl FnOnce()) {
    if registered {
        unregister();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[cfg(unix)]
    #[test]
    fn unix_stop_rolls_back_partial_registration_and_unregisters_on_drop() {
        for fail_at in [Some(0), Some(1), None] {
            let mut attempted = Vec::new();
            let mut installed = Vec::new();
            let result = Stop::register_with(|signal, requested| {
                attempted.push(signal);
                if fail_at == Some(attempted.len() - 1) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "fixture signal registration",
                    ));
                }
                let id = signal_hook::flag::register(signal, requested)?;
                installed.push(id);
                Ok(id)
            });
            match (fail_at, result) {
                (Some(failed), Err(error)) => {
                    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                    assert_eq!(error.to_string(), "fixture signal registration");
                    assert_eq!(attempted, [libc::SIGINT, libc::SIGTERM][..=failed]);
                    assert_eq!(installed.len(), failed);
                }
                (None, Ok(stop)) => {
                    assert_eq!(attempted, [libc::SIGINT, libc::SIGTERM]);
                    assert_eq!(installed.len(), 2);
                    assert!(!stop.requested().load(Relaxed));
                    drop(stop);
                }
                _ => panic!("stop registration returned the wrong outcome"),
            }
            for id in installed {
                assert!(
                    !signal_hook::low_level::unregister(id),
                    "stop registration leaked a handler"
                );
            }
        }
    }

    #[test]
    fn console_registration_requires_attachment_and_preserves_the_failure() {
        let calls = Cell::new(0);
        register_console(
            false,
            || {
                calls.set(calls.get() + 1);
                0
            },
            || panic!("detached console fetched an OS error"),
        )
        .unwrap();
        assert_eq!(calls.get(), 0);
        register_console(
            true,
            || {
                calls.set(calls.get() + 1);
                1
            },
            || panic!("successful console registration fetched an OS error"),
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
        let error = register_console(true, || 0, || io::Error::from_raw_os_error(5)).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5));
    }

    #[test]
    fn console_callback_marks_only_interrupt_and_break_and_tears_down_registration() {
        let stop_events = [0, 1];
        let requested = AtomicBool::new(false);
        assert_eq!(request_console_stop(2, stop_events, &requested), 0);
        assert!(!requested.load(Relaxed));
        for event in stop_events {
            requested.store(false, Relaxed);
            assert_eq!(request_console_stop(event, stop_events, &requested), 1);
            assert!(requested.load(Relaxed));
        }
        unregister_console(false, || panic!("unregistered console removed a callback"));
        let removed = Cell::new(0);
        unregister_console(true, || removed.set(removed.get() + 1));
        assert_eq!(removed.get(), 1);
    }
}
