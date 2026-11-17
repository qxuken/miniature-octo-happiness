//! Private fixture utilities for reading the current Windows user SID.
use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Security::{GetLengthSid, GetTokenInformation, TOKEN_USER, TokenUser},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(crate) fn process_token(access: u32) -> io::Result<OwnedHandle> {
    let mut handle = null_mut();
    // SAFETY: the pseudo-handle is valid and the output owns the new handle.
    if unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut handle) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

pub(crate) fn token_user(token: &OwnedHandle) -> io::Result<[u32; 17]> {
    let mut storage = [0usize; 20];
    let mut bytes = 0;
    let mut sid = [0u32; 17];
    // SAFETY: the aligned buffer holds TOKEN_USER and the largest supported SID.
    // On success the returned SID points into the live completed buffer.
    unsafe {
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            storage.as_mut_ptr().cast(),
            size_of_val(&storage) as u32,
            &mut bytes,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let user = (*storage.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let length = GetLengthSid(user) as usize;
        if length == 0 || length > size_of_val(&sid) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        std::ptr::copy_nonoverlapping(user.cast::<u8>(), sid.as_mut_ptr().cast(), length);
    }
    Ok(sid)
}
