use super::*;
use crate::storage::{IoKind, SimulatedDisk};
struct ReadModel {
    extra: usize,
}
impl ByteStore for ReadModel {
    fn lock(&mut self) -> Result<()> {
        Ok(())
    }
    fn unlock(&mut self) {}
    fn length(&mut self) -> Result<u64> {
        Ok(u64::MAX)
    }
    fn read(&mut self, _: u64, out: &mut [u8]) -> Result<usize> {
        out.fill(0xaa);
        Ok(out.len() + self.extra)
    }
    fn write(&mut self, _: u64, _: &[u8]) -> Result<usize> {
        panic!("source write")
    }
    fn sync(&mut self) -> Result<()> {
        panic!("source sync")
    }
}
#[test]
fn candidate_view_masks_only_unselected_root_and_witness_intersections() {
    for slot in 0..2 {
        let mut source = ReadModel { extra: 0 };
        let mut view = CandidateView {
            source: &mut source,
            slot,
        };
        for (offset, size) in [
            (0, 0),
            (0, REGION * 6),
            (REGION - 1, 2),
            (REGION * 3 - 1, 2),
            (REGION * 5 - 1, 2),
            (REGION * 6, 1),
        ] {
            let mut bytes = vec![0; size];
            assert_eq!(view.read(offset as u64, &mut bytes), Ok(size));
            for (i, b) in bytes.into_iter().enumerate() {
                let region = (offset + i) / REGION;
                assert_eq!(
                    b,
                    if [2 - slot, 3, 4].contains(&region) {
                        0
                    } else {
                        0xaa
                    }
                );
            }
        }
        assert_eq!(view.lock(), Err(Error::Invalid));
        assert_eq!(view.write(0, &[]), Err(Error::Invalid));
        assert_eq!(view.sync(), Err(Error::Invalid));
        assert_eq!(view.read(u64::MAX, &mut [0]), Err(Error::Corrupt));
    }
    let mut source = ReadModel { extra: 1 };
    let mut view = CandidateView {
        source: &mut source,
        slot: 0,
    };
    assert_eq!(view.read(0, &mut [0]), Err(Error::Io));
}
#[test]
fn salvage_later_candidate_and_resource_rejection_preserve_source() {
    let disk = SimulatedDisk::new().unwrap();
    let db = crate::Database::create_on(disk.clone(), [2; 16], Limits::default()).unwrap();
    let c = db.create_collection().unwrap();
    let mut tx = db.begin();
    tx.put(c, b"k", &db.bytes(b"value").unwrap()).unwrap();
    tx.commit().unwrap();
    drop(tx);
    db.retain_snapshot(&db.snapshot()).unwrap();
    db.begin().commit().unwrap();
    db.begin().commit().unwrap();
    assert_eq!(db.snapshot().revision(), 5);
    drop(db);
    let original = disk.durable_image().unwrap();
    let source = SimulatedDisk::from_image(&original).unwrap();
    let dest = SimulatedDisk::new().unwrap();
    let report = salvage_on(source.clone(), dest.clone(), Limits::default()).unwrap();
    assert_eq!(report.revision, 5);
    assert_eq!(report.selected_slot, 1);
    assert_eq!(report.verified_slot_revisions, [Some(4), Some(5)]);
    let restored = crate::Database::open_on(dest, Limits::default()).unwrap();
    assert_eq!(
        restored
            .retained_snapshot(1)
            .unwrap()
            .get(restored.collection_id(1), b"k")
            .unwrap()
            .unwrap()
            .as_bytes()
            .unwrap(),
        b"value"
    );
    assert_eq!(source.durable_image().unwrap(), original);
    assert!(
        source
            .trace()
            .unwrap()
            .iter()
            .all(|e| !matches!(e.kind, IoKind::Write | IoKind::Sync))
    );
    let untouched = SimulatedDisk::new().unwrap();
    assert_eq!(
        salvage_on(
            source.clone(),
            untouched.clone(),
            Limits {
                max_bytes: 1,
                ..Limits::default()
            }
        ),
        Err(Error::Limit)
    );
    assert!(untouched.trace().unwrap().is_empty());
    assert_eq!(source.durable_image().unwrap(), original);
}
