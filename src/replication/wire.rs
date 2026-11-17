use super::{DurableAck, Enrollment, Offer};
use crate::{Error, Result};
use std::io::{Read, Write};

const VERSION: &[u8; 8] = b"WDBREPL\x01";

pub(super) fn request(
    channel: &mut impl Write,
    enrollment: Enrollment,
    offer: Option<Offer>,
) -> Result<()> {
    let mut bytes = [0; 89];
    bytes[..8].copy_from_slice(VERSION);
    bytes[8] = u8::from(offer.is_some());
    bytes[9..25].copy_from_slice(&enrollment.stream_id);
    bytes[25..41].copy_from_slice(&enrollment.file_identity);
    let size = if let Some(offer) = offer {
        bytes[41..49].copy_from_slice(&offer.revision.to_le_bytes());
        bytes[49..57].copy_from_slice(&offer.length.to_le_bytes());
        bytes[57..].copy_from_slice(&offer.digest);
        bytes.len()
    } else {
        41
    };
    write(channel, &bytes[..size])
}

#[cfg(any(test, target_os = "macos", target_os = "linux", windows))]
pub(super) fn read_request(channel: &mut impl Read, expected: Enrollment) -> Result<Option<Offer>> {
    let header = read::<41>(channel)?;
    if &header[..8] != VERSION {
        return Err(Error::UnsupportedFormat);
    }
    if header[9..25] != expected.stream_id || header[25..] != expected.file_identity {
        return Err(Error::ForeignStore);
    }
    match header[8] {
        0 => Ok(None),
        1 => {
            let offer = read::<48>(channel)?;
            let offer = Offer {
                revision: u64::from_le_bytes(offer[..8].try_into().unwrap()),
                length: u64::from_le_bytes(offer[8..16].try_into().unwrap()),
                digest: offer[16..].try_into().unwrap(),
            };
            offer.validate()?;
            Ok(Some(offer))
        }
        _ => Err(Error::Invalid),
    }
}

#[derive(Clone, Copy)]
pub(super) enum Reply {
    Empty,
    Ready,
    Installed(DurableAck),
}

#[cfg(any(test, target_os = "macos", target_os = "linux", windows))]
pub(super) fn reply(channel: &mut impl Write, reply: Result<Reply>) -> Result<()> {
    let mut bytes = [0; 57];
    bytes[0] = match reply {
        Ok(Reply::Empty) => 0,
        Ok(Reply::Ready) => 1,
        Ok(Reply::Installed(ack)) => {
            bytes[1..17].copy_from_slice(&ack.stream_id);
            bytes[17..25].copy_from_slice(&ack.revision.to_le_bytes());
            bytes[25..].copy_from_slice(&ack.digest);
            bytes[0] = 2;
            return write(channel, &bytes);
        }
        Err(Error::UnsupportedFormat) => 3,
        Err(Error::ForeignStore) => 4,
        Err(Error::Conflict) => 5,
        Err(Error::Limit | Error::Exhausted) => 6,
        Err(Error::Corrupt) => 7,
        Err(Error::OutcomeUnknown | Error::RecoveryRequired) => 8,
        Err(Error::Invalid) => 9,
        Err(_) => 10,
    };
    write(channel, &bytes[..1])
}

pub(super) fn read_reply(channel: &mut impl Read) -> Result<Reply> {
    match read::<1>(channel)?[0] {
        0 => Ok(Reply::Empty),
        1 => Ok(Reply::Ready),
        2 => {
            let bytes = read::<56>(channel)?;
            Ok(Reply::Installed(DurableAck {
                stream_id: bytes[..16].try_into().unwrap(),
                revision: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
                digest: bytes[24..].try_into().unwrap(),
            }))
        }
        3 => Err(Error::UnsupportedFormat),
        4 => Err(Error::ForeignStore),
        5 => Err(Error::Conflict),
        6 => Err(Error::Limit),
        7 => Err(Error::Corrupt),
        8 => Err(Error::RecoveryRequired),
        9 => Err(Error::Invalid),
        10 => Err(Error::Io),
        _ => Err(Error::UnsupportedFormat),
    }
}

fn read<const N: usize>(channel: &mut impl Read) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    channel.read_exact(&mut bytes).map_err(|_| Error::Io)?;
    Ok(bytes)
}
fn write(channel: &mut impl Write, bytes: &[u8]) -> Result<()> {
    channel
        .write_all(bytes)
        .and_then(|_| channel.flush())
        .map_err(|_| Error::Io)
}
