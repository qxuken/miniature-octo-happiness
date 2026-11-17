use super::AppResult;
use std::time::Duration;
const SERVER_NAME: &str = "study-peer";
const ALPN: &[u8] = b"study-progress/1";
const ALPN_ERROR: &str = "study ALPN negotiation required";
const TIMEOUT: Duration = Duration::from_secs(3);
#[path = "../../../support/pinned_tls.rs"]
mod implementation;
pub(crate) use implementation::{Credentials, Incoming, Tls, bounded_file};
