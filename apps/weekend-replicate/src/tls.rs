use super::AppResult;
use std::time::Duration;
const SERVER_NAME: &str = "weekend-replicate";
const ALPN: &[u8] = b"weekend-snapshot/1";
const ALPN_ERROR: &str = "snapshot ALPN negotiation required";
const TIMEOUT: Duration = Duration::from_secs(30);
#[path = "../../support/pinned_tls.rs"]
mod implementation;
pub(crate) use implementation::{Credentials, Tls, bounded_file};
