use std::{ffi::c_void, ptr};
use weekend_db::{Database, Error};

// Signatures mirror include/weekend_db.h. Handles are opaque and originate
// exclusively from these exported functions. Every owned handle is released.
unsafe extern "C" {
    fn wdb_open(out: *mut *mut c_void) -> i32;
    fn wdb_close(db: *mut c_void) -> i32;
    fn wdb_value_make(
        db: *mut c_void,
        kind: i32,
        bits: u64,
        bytes: *const u8,
        len: usize,
        out: *mut *mut c_void,
    ) -> i32;
    fn wdb_value_clone(value: *const c_void, out: *mut *mut c_void) -> i32;
    fn wdb_value_release(value: *mut c_void) -> i32;
    fn wdb_value_bytes(
        value: *const c_void,
        buffer: *mut u8,
        capacity: usize,
        required: *mut usize,
    ) -> i32;
    fn wdb_map_set(
        map: *const c_void,
        key: *const u8,
        len: usize,
        value: *const c_void,
        out: *mut *mut c_void,
    ) -> i32;
    fn wdb_map_get(map: *const c_void, key: *const u8, len: usize, out: *mut *mut c_void) -> i32;
}

pub fn ffi_ops(input: &[u8]) {
    let native = Database::new().unwrap();
    let mut owners = [ptr::null_mut(); 2];
    let mut maps = [ptr::null_mut(); 2];
    let mut values = [ptr::null_mut(); 8];
    let mut model: [Option<(usize, Vec<u8>)>; 8] = std::array::from_fn(|_| None);
    // Input controls indices, byte contents, and documented capacities only.
    // Every non-null pointer addresses a live correctly sized allocation.
    unsafe {
        for i in 0..2 {
            assert_eq!(wdb_open(&mut owners[i]), 0);
            assert_eq!(
                wdb_value_make(owners[i], 6, 0, ptr::null(), 0, &mut maps[i]),
                0
            );
        }
        for &[op, slot, byte] in input.as_chunks::<3>().0.iter().take(64) {
            let index = usize::from(slot) % values.len();
            let owner = usize::from(op >> 7);
            match op % 5 {
                0 => {
                    let bytes = vec![byte; usize::from(slot >> 3)];
                    let expected = native.bytes(&bytes).unwrap();
                    let mut value = ptr::null_mut();
                    let pointer = if bytes.is_empty() {
                        ptr::null()
                    } else {
                        bytes.as_ptr()
                    };
                    assert_eq!(
                        wdb_value_make(owners[owner], 5, 0, pointer, bytes.len(), &mut value),
                        0
                    );
                    assert_eq!(wdb_value_release(values[index]), 0);
                    values[index] = value;
                    model[index] = Some((owner, expected.as_bytes().unwrap().to_vec()));
                }
                1 if model[index].is_some() => {
                    let destination = usize::from(byte) % values.len();
                    let mut cloned = ptr::null_mut();
                    assert_eq!(wdb_value_clone(values[index], &mut cloned), 0);
                    let expected = model[index].clone();
                    assert_eq!(wdb_value_release(values[destination]), 0);
                    values[destination] = cloned;
                    model[destination] = expected;
                }
                2 if model[index].is_some() => {
                    let bytes = &model[index].as_ref().unwrap().1;
                    let capacity = usize::from(byte) % (bytes.len() + 2);
                    let mut output = [0xa5; 33];
                    let mut required = usize::MAX;
                    let buffer = if capacity == 0 {
                        ptr::null_mut()
                    } else {
                        output.as_mut_ptr()
                    };
                    let result = wdb_value_bytes(values[index], buffer, capacity, &mut required);
                    assert_eq!(required, bytes.len());
                    if capacity < bytes.len() {
                        assert_eq!(result, Error::Buffer as i32);
                        assert_eq!(output, [0xa5; 33]);
                    } else {
                        assert_eq!(result, 0);
                        assert_eq!(&output[..bytes.len()], bytes);
                        assert!(output[bytes.len()..].iter().all(|b| *b == 0xa5));
                    }
                    assert_eq!(
                        wdb_value_bytes(
                            values[index],
                            output.as_mut_ptr(),
                            output.len(),
                            ptr::null_mut()
                        ),
                        Error::Invalid as i32
                    );
                }
                3 if model[index].is_some() => {
                    let (value_owner, bytes) = model[index].as_ref().unwrap();
                    let mut edited = ptr::null_mut();
                    let result = wdb_map_set(maps[owner], &byte, 1, values[index], &mut edited);
                    if *value_owner != owner {
                        assert_eq!(result, Error::ForeignStore as i32);
                        assert!(edited.is_null());
                    } else {
                        assert_eq!(result, 0);
                        let mut fetched = ptr::null_mut();
                        assert_eq!(wdb_map_get(edited, &byte, 1, &mut fetched), 0);
                        let mut output = [0; 32];
                        let mut length = 0;
                        assert_eq!(
                            wdb_value_bytes(
                                fetched,
                                output.as_mut_ptr(),
                                output.len(),
                                &mut length
                            ),
                            0
                        );
                        assert_eq!(&output[..length], bytes);
                        assert_eq!(wdb_value_release(fetched), 0);
                        assert_eq!(wdb_value_release(maps[owner]), 0);
                        maps[owner] = edited;
                    }
                }
                4 => {
                    assert_eq!(wdb_value_release(values[index]), 0);
                    values[index] = ptr::null_mut();
                    model[index] = None;
                }
                _ => {}
            }
            // Both owners always have at least their map handle.
            assert_eq!(wdb_close(owners[owner]), Error::Busy as i32);
        }
        for value in values.into_iter().chain(maps) {
            assert_eq!(wdb_value_release(value), 0);
        }
        for db in owners {
            assert_eq!(wdb_close(db), 0);
        }
    }
}
