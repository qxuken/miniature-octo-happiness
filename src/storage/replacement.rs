//! Namespace publication after a replacement image is fully durable/verified.
//! An atomic exchange and ordered namespace persistence are adapter assumptions.
pub(crate) trait Replacement {
    fn exchange(&mut self) -> crate::Result<()>;
    fn sync_directory(&mut self) -> crate::Result<()>;
    fn sync_device(&mut self) -> crate::Result<()>;
    fn remove_old(&mut self) -> crate::Result<()>;
}
pub(crate) fn publish(fs: &mut impl Replacement) -> crate::Result<()> {
    fs.exchange()?;
    fs.sync_directory()?;
    fs.sync_device()?;
    fs.remove_old()?;
    fs.sync_directory()?;
    fs.sync_device()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, Error, Limits, storage::SimulatedDisk};
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Names {
        canonical: Option<usize>,
        scratch: Option<usize>,
    }
    struct Namespace {
        durable: Names,
        flushed: Names,
        volatile: Names,
        boundary: usize,
        fail: usize,
    }
    impl Namespace {
        fn step(&mut self, action: impl FnOnce(&mut Self)) -> crate::Result<()> {
            if self.boundary == self.fail {
                return Err(Error::Io);
            }
            self.boundary += 1;
            action(self);
            if self.boundary == self.fail {
                return Err(Error::Io);
            }
            self.boundary += 1;
            Ok(())
        }
    }
    impl Replacement for Namespace {
        fn exchange(&mut self) -> crate::Result<()> {
            self.step(|s| std::mem::swap(&mut s.volatile.canonical, &mut s.volatile.scratch))
        }
        fn sync_directory(&mut self) -> crate::Result<()> {
            self.step(|s| s.flushed = s.volatile)
        }
        fn sync_device(&mut self) -> crate::Result<()> {
            self.step(|s| s.durable = s.flushed)
        }
        fn remove_old(&mut self) -> crate::Result<()> {
            self.step(|s| s.volatile.scratch = None)
        }
    }
    #[test]
    fn power_loss_at_every_namespace_boundary_keeps_a_complete_canonical_store() {
        let disk = SimulatedDisk::new().unwrap();
        let mut db =
            Database::create_on(disk.clone(), *b"namespace-test-1", Limits::default()).unwrap();
        let c = db.create_collection().unwrap();
        for n in 0..12 {
            let mut tx = db.begin();
            tx.put(c, b"n", &db.unsigned(n).unwrap()).unwrap();
            tx.commit().unwrap();
            drop(tx);
            if n == 0 {
                db.retain_snapshot(&db.snapshot()).unwrap();
            }
        }
        let revision = db.snapshot().revision();
        let target = SimulatedDisk::new().unwrap();
        db.compact_to(target.clone()).unwrap();
        let images = [
            disk.durable_image().unwrap(),
            target.durable_image().unwrap(),
        ];
        for fail in 0..=12 {
            let initial = Names {
                canonical: Some(0),
                scratch: Some(1),
            };
            let mut namespace = Namespace {
                durable: initial,
                flushed: initial,
                volatile: initial,
                boundary: 0,
                fail,
            };
            let result = publish(&mut namespace);
            for (policy, names) in [namespace.durable, namespace.flushed, namespace.volatile]
                .into_iter()
                .enumerate()
            {
                let index = names.canonical.expect("crash lost canonical entry");
                let reopened = Database::open_on(
                    SimulatedDisk::from_image(&images[index]).unwrap(),
                    Limits::default(),
                )
                .unwrap();
                let c = reopened.collection_id(c.as_u64());
                assert_eq!(
                    reopened.snapshot().revision(),
                    revision,
                    "fail={fail}, policy={policy}"
                );
                assert_eq!(
                    reopened.snapshot().get(c, b"n").unwrap().unwrap().as_u64(),
                    Ok(11)
                );
                assert_eq!(
                    reopened
                        .retained_snapshot(1)
                        .unwrap()
                        .get(c, b"n")
                        .unwrap()
                        .unwrap()
                        .as_u64(),
                    Ok(0)
                );
                if result.is_ok() {
                    assert_eq!(names.canonical, Some(1));
                    assert_eq!(names.scratch, None);
                }
            }
        }
    }
}
