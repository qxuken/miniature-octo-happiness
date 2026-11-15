//! Restrict the fixture's current thread before opening any measured file.
#[path = "../../quota/windows_identity.rs"]
mod identity;

use std::{
    io,
    marker::PhantomData,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::Win32::Security::{
    CheckTokenMembership, CreateRestrictedToken, CreateWellKnownSid, DISABLE_MAX_PRIVILEGE,
    ImpersonateLoggedOnUser, RevertToSelf, SID_AND_ATTRIBUTES, SetTokenInformation,
    TOKEN_ADJUST_DEFAULT, TOKEN_DUPLICATE, TOKEN_OWNER, TOKEN_QUERY, TokenOwner,
    WinBuiltinAdministratorsSid,
};

pub(super) struct Restricted(PhantomData<Rc<()>>);

impl Restricted {
    pub(super) fn enter() -> io::Result<Self> {
        let primary =
            identity::process_token(TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ADJUST_DEFAULT)?;
        let mut user = identity::token_user(&primary)?;
        let mut admins = [0u32; 17];
        let mut bytes = size_of_val(&admins) as u32;
        let mut restricted = null_mut();
        // SAFETY: all input buffers are live and aligned and output pointers are writable.
        // The returned token handle is owned and installed only for this thread.
        unsafe {
            if CreateWellKnownSid(
                WinBuiltinAdministratorsSid,
                null_mut(),
                admins.as_mut_ptr().cast(),
                &mut bytes,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let disable = SID_AND_ATTRIBUTES {
                Sid: admins.as_mut_ptr().cast(),
                Attributes: 0,
            };
            if CreateRestrictedToken(
                primary.as_raw_handle(),
                DISABLE_MAX_PRIVILEGE,
                1,
                &disable,
                0,
                null(),
                0,
                null(),
                &mut restricted,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let restricted = OwnedHandle::from_raw_handle(restricted);
            let owner = TOKEN_OWNER {
                Owner: user.as_mut_ptr().cast(),
            };
            if SetTokenInformation(
                restricted.as_raw_handle(),
                TokenOwner,
                (&raw const owner).cast(),
                size_of_val(&owner) as u32,
            ) == 0
                || ImpersonateLoggedOnUser(restricted.as_raw_handle()) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let guard = Self(PhantomData);
            let mut is_admin = 0;
            if CheckTokenMembership(null_mut(), admins.as_mut_ptr().cast(), &mut is_admin) == 0 {
                return Err(io::Error::last_os_error());
            }
            if is_admin != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "fixture token is still an administrator",
                ));
            }
            Ok(guard)
        }
    }
}

impl Drop for Restricted {
    fn drop(&mut self) {
        // SAFETY: the guard stays on the impersonating thread.
        if unsafe { RevertToSelf() } == 0 {
            std::process::abort();
        }
    }
}
