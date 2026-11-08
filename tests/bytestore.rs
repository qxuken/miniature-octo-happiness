use weekend_db::{
    Error,
    storage::{ByteStore, Crash, Fault, MAX_IMAGE_BYTES, SimulatedDisk},
};

#[test]
fn lease_clones_crashes_and_stale_unlocks_preserve_exclusion() {
    let mut first = SimulatedDisk::new().unwrap();
    let mut second = first.clone();
    assert_eq!(first.length(), Err(Error::Io));
    first.lock().unwrap();
    assert_eq!(first.length(), Ok(0));
    assert_eq!(second.lock(), Err(Error::Busy));
    assert_eq!(second.length(), Err(Error::Io));
    second.unlock();
    assert_eq!(first.length(), Ok(0));
    first.crash(Crash::DropAll).unwrap();
    assert_eq!(first.length(), Err(Error::Io));
    second.lock().unwrap();
    assert_eq!(first.length(), Err(Error::Io));
    first.unlock();
    assert_eq!(second.length(), Ok(0));
    assert_eq!(first.lock(), Err(Error::Busy));
    second.unlock();
    first.lock().unwrap();
    assert_eq!(first.length(), Ok(0));
}

#[test]
fn crash_policies_cover_writes_beyond_the_mask_word_and_prefix_bounds() {
    for (policy, expected) in [
        (Crash::DropAll, vec![0; 66]),
        (Crash::PersistAll, vec![1; 66]),
        (Crash::Mask(u64::MAX), [vec![1; 64], vec![0; 2]].concat()),
        (
            Crash::Torn {
                write: 64,
                prefix: usize::MAX,
            },
            {
                let mut bytes = vec![0; 66];
                bytes[64] = 1;
                bytes
            },
        ),
        (
            Crash::Torn {
                write: 64,
                prefix: 0,
            },
            vec![0; 66],
        ),
        (
            Crash::Torn {
                write: 66,
                prefix: 1,
            },
            vec![0; 66],
        ),
    ] {
        let mut disk = SimulatedDisk::from_image(&[0; 66]).unwrap();
        disk.lock().unwrap();
        for offset in 0..66 {
            assert_eq!(disk.write(offset, &[1]), Ok(1));
        }
        disk.crash(policy).unwrap();
        assert_eq!(disk.durable_image().unwrap(), expected);
    }
    let mut disk = SimulatedDisk::from_image(&[0]).unwrap();
    disk.lock().unwrap();
    disk.write(0, &[1]).unwrap();
    disk.write(0, &[2]).unwrap();
    disk.crash(Crash::Mask(3)).unwrap();
    assert_eq!(
        disk.durable_image().unwrap(),
        [1],
        "masked writes survive in reverse call order"
    );
}

#[test]
fn byte_ranges_and_one_shot_faults_leave_a_usable_lease() {
    let mut disk = SimulatedDisk::new().unwrap();
    disk.lock().unwrap();
    assert_eq!(disk.read(MAX_IMAGE_BYTES as u64, &mut []), Ok(0));
    assert_eq!(
        disk.read(MAX_IMAGE_BYTES as u64, &mut [0]),
        Err(Error::Limit)
    );
    assert_eq!(disk.write(u64::MAX, &[1]), Err(Error::Limit));
    assert_eq!(
        disk.write(MAX_IMAGE_BYTES as u64 + 1, &[]),
        Err(Error::Limit)
    );
    assert_eq!(disk.write(0, &[1, 2]), Ok(2));
    disk.arm(0, Fault::Short(1));
    assert_eq!(disk.read(0, &mut [0; 2]), Ok(1));
    assert_eq!(disk.read(0, &mut [0; 2]), Ok(2));
    disk.arm(0, Fault::Error);
    assert_eq!(disk.sync(), Err(Error::Io));
    assert_eq!(disk.durable_image().unwrap(), []);
    disk.sync().unwrap();
    assert_eq!(disk.durable_image().unwrap(), [1, 2]);
}
