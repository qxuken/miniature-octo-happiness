use std::{io, net::TcpListener};

#[cfg(unix)]
pub(crate) fn wait_for_connection(listener: &TcpListener, wait_ms: i32) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    let mut descriptor = libc::pollfd {
        fd: listener.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: the listener stays open and descriptor holds one writable pollfd.
    let ready = unsafe { libc::poll(&mut descriptor, 1, wait_ms) };
    poll_result(ready, io::Error::last_os_error)
}

#[cfg(any(unix, test))]
fn poll_result(ready: i32, last_error: impl FnOnce() -> io::Error) -> io::Result<bool> {
    if ready < 0 {
        Err(last_error())
    } else {
        Ok(ready != 0)
    }
}

#[cfg(windows)]
pub(crate) fn wait_for_connection(listener: &TcpListener, wait_ms: i32) -> io::Result<bool> {
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
    let ready = unsafe { WSAPoll(&mut descriptor, 1, wait_ms) };
    winsock_poll_result(ready, SOCKET_ERROR, || {
        // SAFETY: called on the same thread that made the failed Winsock call.
        io::Error::from_raw_os_error(unsafe { WSAGetLastError() })
    })
}

#[cfg(any(windows, test))]
fn winsock_poll_result(
    ready: i32,
    socket_error: i32,
    last_error: impl FnOnce() -> io::Error,
) -> io::Result<bool> {
    if ready == socket_error {
        Err(last_error())
    } else {
        Ok(ready != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, net::TcpStream};

    #[test]
    fn native_listener_distinguishes_idle_and_ready() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(!wait_for_connection(&listener, 0).unwrap());
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        assert!(wait_for_connection(&listener, 3_000).unwrap());
        let (socket, _) = listener.accept().unwrap();
        assert!(!wait_for_connection(&listener, 0).unwrap());
        drop((peer, socket));
    }

    #[test]
    fn poll_result_preserves_an_error_and_reads_it_only_on_failure() {
        assert!(!poll_result(0, || panic!("idle poll read its OS error")).unwrap());
        assert!(poll_result(1, || panic!("ready poll read its OS error")).unwrap());
        let calls = Cell::new(0);
        let error = poll_result(-1, || {
            calls.set(calls.get() + 1);
            io::Error::new(io::ErrorKind::Interrupted, "fixture poll interruption")
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(error.to_string(), "fixture poll interruption");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn winsock_poll_result_preserves_raw_error_and_skips_it_on_success() {
        const SOCKET_ERROR: i32 = -1;
        assert!(
            !winsock_poll_result(0, SOCKET_ERROR, || panic!("idle WSAPoll read its OS error"))
                .unwrap()
        );
        assert!(
            winsock_poll_result(1, SOCKET_ERROR, || panic!(
                "ready WSAPoll read its OS error"
            ))
            .unwrap()
        );
        let calls = Cell::new(0);
        let error = winsock_poll_result(SOCKET_ERROR, SOCKET_ERROR, || {
            calls.set(calls.get() + 1);
            io::Error::from_raw_os_error(10_004)
        })
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(10_004));
        assert_eq!(calls.get(), 1);
    }
}
