//! Packaging probe only. All non-null FFI pointers must be live, aligned, and
//! valid for the stated length/type. Output storage must not alias inputs.
//! Handles are single-thread-owned. See ../README.md for the shared contract.
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::missing_safety_doc)]

use std::{cell::Cell, panic::AssertUnwindSafe, ptr, rc::Rc, slice};

const OK: i32 = 0;
const INVALID: i32 = 1;
const TYPE: i32 = 2;
const NOT_FOUND: i32 = 3;
const BOUNDS: i32 = 4;
const BUFFER: i32 = 5;
const BUSY: i32 = 6;
const LIMIT: i32 = 7;
const PANIC: i32 = 9;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_DEPTH: usize = 64;

#[derive(Clone)]
enum Data {
    Bytes(Vec<u8>),
    Map(Vec<(Vec<u8>, Rc<Node>)>),
    List(Vec<Rc<Node>>),
}
struct Node {
    data: Data,
    depth: usize,
}
struct Owner {
    handles: Cell<usize>,
}
pub struct Db {
    owner: Rc<Owner>,
}
pub struct Value {
    owner: Rc<Owner>,
    node: Rc<Node>,
}
impl Drop for Value {
    fn drop(&mut self) {
        self.owner.handles.set(self.owner.handles.get() - 1);
    }
}
fn boundary(f: impl FnOnce() -> i32) -> i32 {
    std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or(PANIC)
}
unsafe fn input<'a>(bytes: *const u8, len: usize) -> Result<&'a [u8], i32> {
    if len > MAX_BYTES {
        return Err(LIMIT);
    }
    if len == 0 {
        return Ok(&[]);
    }
    if bytes.is_null() {
        return Err(INVALID);
    }
    Ok(unsafe { slice::from_raw_parts(bytes, len) })
}
unsafe fn output<T>(out: *mut *mut T) -> Result<(), i32> {
    if out.is_null() {
        return Err(INVALID);
    }
    unsafe { *out = ptr::null_mut() };
    Ok(())
}
unsafe fn wrap(owner: &Rc<Owner>, node: Rc<Node>, out: *mut *mut Value) -> i32 {
    let Some(count) = owner.handles.get().checked_add(1) else {
        return LIMIT;
    };
    let value = Box::new(Value {
        owner: owner.clone(),
        node,
    });
    owner.handles.set(count);
    unsafe { *out = Box::into_raw(value) };
    OK
}
// Convert validation results inside a status-returning FFI boundary.
macro_rules! check {
    ($expr:expr) => {
        match $expr {
            Ok(value) => value,
            Err(status) => return status,
        }
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_open(out: *mut *mut Db) -> i32 {
    boundary(|| unsafe {
        check!(output(out));
        *out = Box::into_raw(Box::new(Db {
            owner: Rc::new(Owner {
                handles: Cell::new(0),
            }),
        }));
        OK
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_close(db: *mut Db) -> i32 {
    boundary(|| unsafe {
        let Some(db_ref) = db.as_ref() else {
            return INVALID;
        };
        if db_ref.owner.handles.get() != 0 {
            return BUSY;
        }
        drop(Box::from_raw(db));
        OK
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_make(
    db: *mut Db,
    kind: i32,
    bytes: *const u8,
    len: usize,
    out: *mut *mut Value,
) -> i32 {
    boundary(|| unsafe {
        check!(output(out));
        let Some(db) = db.as_ref() else {
            return INVALID;
        };
        if !matches!(kind, 1..=3) || (bytes.is_null() && len != 0) {
            return INVALID;
        }
        let bytes = check!(input(bytes, len));
        if kind != 1 && len != 0 {
            return INVALID;
        }
        let data = match kind {
            1 => Data::Bytes(bytes.to_vec()),
            2 => Data::Map(Vec::new()),
            _ => Data::List(Vec::new()),
        };
        wrap(&db.owner, Rc::new(Node { data, depth: 1 }), out)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_clone(value: *const Value, out: *mut *mut Value) -> i32 {
    boundary(|| unsafe {
        check!(output(out));
        let Some(value) = value.as_ref() else {
            return INVALID;
        };
        wrap(&value.owner, value.node.clone(), out)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_release(value: *mut Value) {
    if !value.is_null() {
        unsafe { drop(Box::from_raw(value)) };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_map_set(
    map: *const Value,
    key: *const u8,
    len: usize,
    value: *const Value,
    out: *mut *mut Value,
) -> i32 {
    boundary(|| unsafe {
        check!(output(out));
        let (Some(map), Some(value)) = (map.as_ref(), value.as_ref()) else {
            return INVALID;
        };
        if !Rc::ptr_eq(&map.owner, &value.owner) || (key.is_null() && len != 0) {
            return INVALID;
        }
        let Data::Map(entries) = &map.node.data else {
            return TYPE;
        };
        let key = check!(input(key, len));
        if value.node.depth >= MAX_DEPTH {
            return LIMIT;
        }
        let index = entries.iter().position(|(k, _)| k == key);
        if index.is_none() && entries.len() >= MAX_ENTRIES {
            return LIMIT;
        }
        let mut entries = entries.clone();
        if let Some(index) = index {
            entries[index].1 = value.node.clone();
        } else {
            entries.push((key.to_vec(), value.node.clone()));
        }
        let depth = entries.iter().map(|(_, v)| v.depth + 1).max().unwrap_or(1);
        wrap(
            &map.owner,
            Rc::new(Node {
                data: Data::Map(entries),
                depth,
            }),
            out,
        )
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_map_get(
    map: *const Value,
    key: *const u8,
    len: usize,
    out: *mut *mut Value,
) -> i32 {
    boundary(|| unsafe {
        check!(output(out));
        let Some(map) = map.as_ref() else {
            return INVALID;
        };
        if key.is_null() && len != 0 {
            return INVALID;
        }
        let Data::Map(entries) = &map.node.data else {
            return TYPE;
        };
        let key = check!(input(key, len));
        let Some((_, node)) = entries.iter().find(|(k, _)| k == key) else {
            return NOT_FOUND;
        };
        wrap(&map.owner, node.clone(), out)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_list_push(
    list: *const Value,
    value: *const Value,
    out: *mut *mut Value,
) -> i32 {
    boundary(|| unsafe {
        check!(output(out));
        let (Some(list), Some(value)) = (list.as_ref(), value.as_ref()) else {
            return INVALID;
        };
        if !Rc::ptr_eq(&list.owner, &value.owner) {
            return INVALID;
        }
        let Data::List(entries) = &list.node.data else {
            return TYPE;
        };
        if entries.len() >= MAX_ENTRIES || value.node.depth >= MAX_DEPTH {
            return LIMIT;
        }
        let mut entries = entries.clone();
        entries.push(value.node.clone());
        let depth = list.node.depth.max(value.node.depth + 1);
        wrap(
            &list.owner,
            Rc::new(Node {
                data: Data::List(entries),
                depth,
            }),
            out,
        )
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_list_get(
    list: *const Value,
    index: usize,
    out: *mut *mut Value,
) -> i32 {
    boundary(|| unsafe {
        check!(output(out));
        let Some(list) = list.as_ref() else {
            return INVALID;
        };
        let Data::List(entries) = &list.node.data else {
            return TYPE;
        };
        let Some(node) = entries.get(index) else {
            return BOUNDS;
        };
        wrap(&list.owner, node.clone(), out)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wp_read_bytes(
    value: *const Value,
    buffer: *mut u8,
    capacity: usize,
    required: *mut usize,
) -> i32 {
    boundary(|| unsafe {
        if required.is_null() {
            return INVALID;
        }
        *required = 0;
        let Some(value) = value.as_ref() else {
            return INVALID;
        };
        if buffer.is_null() && capacity != 0 {
            return INVALID;
        }
        let Data::Bytes(bytes) = &value.node.data else {
            return TYPE;
        };
        *required = bytes.len();
        if capacity < bytes.len() {
            return BUFFER;
        }
        if !bytes.is_empty() {
            ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
        }
        OK
    })
}
