//! Small lifetime-checked consumer of the same C ABI for either implementation.
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};
type Raw = c_void;
unsafe extern "C" {
    fn wp_open(out: *mut *mut Raw) -> i32;
    fn wp_close(db: *mut Raw) -> i32;
    fn wp_make(db: *mut Raw, kind: i32, bytes: *const u8, len: usize, out: *mut *mut Raw) -> i32;
    fn wp_clone(value: *const Raw, out: *mut *mut Raw) -> i32;
    fn wp_release(value: *mut Raw);
    fn wp_map_set(
        map: *const Raw,
        key: *const u8,
        len: usize,
        value: *const Raw,
        out: *mut *mut Raw,
    ) -> i32;
    fn wp_map_get(map: *const Raw, key: *const u8, len: usize, out: *mut *mut Raw) -> i32;
    fn wp_list_push(list: *const Raw, value: *const Raw, out: *mut *mut Raw) -> i32;
    fn wp_list_get(list: *const Raw, index: usize, out: *mut *mut Raw) -> i32;
    fn wp_read_bytes(
        value: *const Raw,
        buffer: *mut u8,
        capacity: usize,
        required: *mut usize,
    ) -> i32;
}
struct Db {
    raw: NonNull<Raw>,
    _single_thread: PhantomData<Rc<()>>,
}
struct Value<'db> {
    raw: NonNull<Raw>,
    db: &'db Db,
}
fn status(code: i32) -> Result<(), i32> {
    if code == 0 { Ok(()) } else { Err(code) }
}
impl Db {
    fn open() -> Result<Self, i32> {
        let mut raw = std::ptr::null_mut();
        status(unsafe { wp_open(&mut raw) })?;
        Ok(Self {
            raw: NonNull::new(raw).unwrap(),
            _single_thread: PhantomData,
        })
    }
    fn value(&self, f: impl FnOnce(*mut *mut Raw) -> i32) -> Result<Value<'_>, i32> {
        let mut raw = std::ptr::null_mut();
        status(f(&mut raw))?;
        Ok(Value {
            raw: NonNull::new(raw).unwrap(),
            db: self,
        })
    }
    fn make(&self, kind: i32, bytes: &[u8]) -> Result<Value<'_>, i32> {
        self.value(|out| unsafe {
            wp_make(self.raw.as_ptr(), kind, bytes.as_ptr(), bytes.len(), out)
        })
    }
}
impl Drop for Db {
    fn drop(&mut self) {
        assert_eq!(unsafe { wp_close(self.raw.as_ptr()) }, 0);
    }
}
impl<'db> Value<'db> {
    fn try_clone(&self) -> Result<Self, i32> {
        self.db
            .value(|out| unsafe { wp_clone(self.raw.as_ptr(), out) })
    }
    fn set(&self, key: &[u8], value: &Self) -> Result<Self, i32> {
        self.db.value(|out| unsafe {
            wp_map_set(
                self.raw.as_ptr(),
                key.as_ptr(),
                key.len(),
                value.raw.as_ptr(),
                out,
            )
        })
    }
    fn get(&self, key: &[u8]) -> Result<Self, i32> {
        self.db
            .value(|out| unsafe { wp_map_get(self.raw.as_ptr(), key.as_ptr(), key.len(), out) })
    }
    fn push(&self, value: &Self) -> Result<Self, i32> {
        self.db
            .value(|out| unsafe { wp_list_push(self.raw.as_ptr(), value.raw.as_ptr(), out) })
    }
    fn at(&self, index: usize) -> Result<Self, i32> {
        self.db
            .value(|out| unsafe { wp_list_get(self.raw.as_ptr(), index, out) })
    }
    fn bytes(&self) -> Result<Vec<u8>, i32> {
        let mut len = 0;
        let code = unsafe { wp_read_bytes(self.raw.as_ptr(), std::ptr::null_mut(), 0, &mut len) };
        if code != 0 && code != 5 {
            return Err(code);
        }
        let mut bytes = vec![0; len];
        status(unsafe { wp_read_bytes(self.raw.as_ptr(), bytes.as_mut_ptr(), len, &mut len) })?;
        Ok(bytes)
    }
}
impl Drop for Value<'_> {
    fn drop(&mut self) {
        unsafe { wp_release(self.raw.as_ptr()) }
    }
}
#[cfg(not(lifetime_mistake))]
fn main() -> Result<(), i32> {
    let db = Db::open()?;
    let bytes = db.make(1, b"a\0b")?;
    let list = db.make(3, b"")?;
    let map = db.make(2, b"")?;
    let tags = list.push(&bytes)?;
    let old = map.set(b"tags", &tags)?;
    let clone = old.try_clone()?;
    let edited = clone.set(b"tags", &tags.push(&bytes)?)?;
    drop(old);
    drop(tags);
    drop(bytes);
    drop(list);
    drop(map);
    assert_eq!(clone.get(b"tags")?.at(0)?.bytes()?, b"a\0b");
    assert!(matches!(clone.get(b"tags")?.at(1), Err(4)));
    assert_eq!(edited.get(b"tags")?.at(1)?.bytes()?, b"a\0b");
    assert!(matches!(clone.get(b"missing"), Err(3)));
    println!("Rust consumer: ok");
    Ok(())
}

// Deliberately rejected by rustc: a value cannot escape its owning database.
#[cfg(lifetime_mistake)]
fn main() {
    let escaped;
    {
        let db = Db::open().unwrap();
        escaped = db.make(1, b"expired").unwrap();
    }
    println!("{:?}", escaped.bytes());
}
