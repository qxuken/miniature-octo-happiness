use std::path::Path;
use weekend_db::{
    Database, Error, Limits, Result,
    replication::{Enrollment, ExportDescriptor},
};

const KEY: &[u8] = b"snapshot-capture";
const FORMAT: &[u8; 8] = b"WDBCAP01";

pub(super) fn store(path: &Path, descriptor: ExportDescriptor) -> Result<()> {
    let mut bytes = [0; 88];
    bytes[..8].copy_from_slice(FORMAT);
    bytes[8..24].copy_from_slice(&descriptor.enrollment.stream_id);
    bytes[24..40].copy_from_slice(&descriptor.enrollment.file_identity);
    bytes[40..48].copy_from_slice(&descriptor.revision.to_le_bytes());
    bytes[48..56].copy_from_slice(&descriptor.length.to_le_bytes());
    bytes[56..88].copy_from_slice(&descriptor.digest);
    // The creation commit makes the directory entry durable before a
    // subsequent commit can expose the descriptor. Normal open stabilizes
    // a selected row after an interrupted descriptor commit.
    let record = Database::create_file(path, Limits::default())?;
    let collection = record.create_collection()?;
    let mut tx = record.begin();
    tx.put(collection, KEY, &record.bytes(&bytes)?)?;
    tx.commit()?;
    Ok(())
}

pub(super) fn load(path: &Path) -> Result<ExportDescriptor> {
    let record = Database::open_file(path, Limits::default())?;
    let row = record
        .snapshot()
        .get(record.collection_id(1), KEY)
        .map_err(|error| {
            if error == Error::NotFound {
                Error::UnsupportedFormat
            } else {
                error
            }
        })?
        .ok_or(Error::UnsupportedFormat)?;
    let bytes = row.as_bytes().map_err(|_| Error::Corrupt)?;
    if bytes.len() != 88 || &bytes[..8] != FORMAT {
        return Err(Error::Corrupt);
    }
    Ok(ExportDescriptor {
        enrollment: Enrollment {
            stream_id: bytes[8..24].try_into().unwrap(),
            file_identity: bytes[24..40].try_into().unwrap(),
        },
        revision: u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
        length: u64::from_le_bytes(bytes[48..56].try_into().unwrap()),
        digest: bytes[56..88].try_into().unwrap(),
    })
}
