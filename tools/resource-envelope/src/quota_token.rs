//! Set the default owner of the dedicated maintenance fixture process.
#[path = "../../quota/windows_identity.rs"]
mod identity;

use std::{io, os::windows::io::AsRawHandle};
use windows_sys::Win32::Security::{
    SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_OWNER, TOKEN_QUERY, TokenOwner,
};

pub(super) fn set_copy_owner() -> io::Result<()> {
    let token = identity::process_token(TOKEN_QUERY | TOKEN_ADJUST_DEFAULT)?;
    let mut user = identity::token_user(&token)?;
    let owner = TOKEN_OWNER {
        Owner: user.as_mut_ptr().cast(),
    };
    // SAFETY: both the descriptor and its SID live through this synchronous call.
    if unsafe {
        SetTokenInformation(
            token.as_raw_handle(),
            TokenOwner,
            (&raw const owner).cast(),
            size_of_val(&owner) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
