//! Collect a nonzero Windows file identity from complete RNG draws.
use super::{NativeIoOperation, diagnostics};
use crate::{Error, Result};
use std::io;

pub(super) fn identity_with(mut fill: impl FnMut(&mut [u8; 16]) -> i32) -> Result<[u8; 16]> {
    for _ in 0..3 {
        let mut id = [0; 16];
        let status = fill(&mut id);
        if status != 0 {
            diagnostics::record(
                NativeIoOperation::Random,
                &io::Error::from_raw_os_error(status),
            );
            return Err(Error::Io);
        }
        if id != [0; 16] {
            return Ok(id);
        }
    }
    Err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_accepts_any_of_three_complete_nonzero_attempts() {
        for success in [1, 2, 3] {
            let mut calls = 0;
            let mut expected = [0; 16];
            expected[15] = 7;
            assert_eq!(
                identity_with(|id| {
                    calls += 1;
                    assert!(calls <= success, "continued after success");
                    assert_eq!(*id, [0; 16]);
                    if calls == success {
                        id[15] = 7;
                    }
                    0
                }),
                Ok(expected)
            );
            assert_eq!(calls, success);
        }
    }

    #[test]
    fn identity_rejects_exactly_three_complete_zero_attempts() {
        let mut calls = 0;
        let failure = diagnostics::capture(|| {
            identity_with(|id| {
                calls += 1;
                assert_eq!(*id, [0; 16]);
                0
            })
        })
        .unwrap_err();
        assert_eq!(failure.error, Error::Io);
        assert!(failure.native.is_none());
        assert_eq!(calls, 3);
    }

    #[test]
    fn identity_rejects_every_nonzero_status_before_accepting_bytes() {
        for status in [0, 1, 0xc000_0001u32 as i32] {
            for preceding_zeros in [0, 2] {
                let mut calls = 0;
                let result = diagnostics::capture(|| {
                    identity_with(|id| {
                        calls += 1;
                        assert_eq!(*id, [0; 16]);
                        if calls <= preceding_zeros {
                            0
                        } else {
                            id[15] = 7;
                            status
                        }
                    })
                });
                assert_eq!(calls, preceding_zeros + 1);
                if status == 0 {
                    let mut expected = [0; 16];
                    expected[15] = 7;
                    assert_eq!(result.unwrap(), expected);
                } else {
                    let failure = result.unwrap_err();
                    assert_eq!(failure.error, Error::Io);
                    let native = failure.native.unwrap();
                    assert_eq!(native.operation, NativeIoOperation::Random);
                    assert_eq!(native.raw_os_error, Some(status));
                }
            }
        }
    }
}
