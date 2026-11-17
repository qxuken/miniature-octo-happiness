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
    if ready < 0 {
        Err(io::Error::last_os_error())
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
    if ready == SOCKET_ERROR {
        // SAFETY: called on the same thread that made the failed Winsock call.
        Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }))
    } else {
        Ok(ready != 0)
    }
}
