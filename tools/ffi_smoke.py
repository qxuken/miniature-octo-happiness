#!/usr/bin/env python3
"""Load the actual shared library and exercise its C ABI, including on Windows."""
import ctypes as c
from pathlib import Path
import sys
import os
import platform
import tempfile


def main():
    lib = c.CDLL(str(Path(sys.argv[1]).resolve()))
    ptr, u64, size = c.c_void_p, c.c_uint64, c.c_size_t

    def call(name, types, *args, status=0):
        function = getattr(lib, "wdb_" + name)
        function.argtypes, function.restype = types, c.c_int32
        result = function(*args)
        assert result == status, (name, result, status)

    db, tx, value, snapshot, found = (ptr() for _ in range(5))
    collection, revision, bits = (u64() for _ in range(3))
    call("open", [c.POINTER(ptr)], c.byref(db))
    call("collection_create", [ptr, c.POINTER(u64)], db, c.byref(collection))
    call("value_make", [ptr, c.c_int32, u64, ptr, size, c.POINTER(ptr)],
         db, 3, 123, None, 0, c.byref(value))
    call("tx_begin", [ptr, c.c_int32, c.POINTER(ptr)], db, 0, c.byref(tx))
    key = c.create_string_buffer(b"binary\0key")
    call("tx_put", [ptr, u64, ptr, size, ptr], tx, collection, key, 10, value)
    call("tx_commit", [ptr, c.POINTER(u64)], tx, c.byref(revision))
    assert revision.value == 2
    call("tx_release", [ptr], tx)
    call("snapshot_acquire", [ptr, c.POINTER(ptr)], db, c.byref(snapshot))
    call("snapshot_get", [ptr, u64, ptr, size, c.POINTER(ptr)],
         snapshot, collection, key, 10, c.byref(found))
    call("value_scalar", [ptr, c.POINTER(u64)], found, c.byref(bits))
    assert bits.value == 123
    call("close", [ptr], db, status=6)
    call("value_release", [ptr], found)
    call("value_release", [ptr], value)
    call("snapshot_release", [ptr], snapshot)
    call("close", [ptr], db)
    can_replace = (sys.platform == "darwin" or
                   (sys.platform == "linux" and platform.machine() in ("x86_64", "aarch64")))
    if not can_replace and sys.platform != "win32":
        call("file_open", [ptr, size, c.POINTER(ptr)], key, 10, c.byref(db), status=20)
        assert db.value is None
        before, after = u64(1), u64(1)
        call("file_compact", [ptr, size, c.POINTER(u64), c.POINTER(u64)],
             key, 10, c.byref(before), c.byref(after), status=20)
        assert before.value == after.value == 0
    else:
        with tempfile.TemporaryDirectory(prefix="weekend-ffi-") as directory:
            path = Path(directory) / "native-λ.db"
            path_bytes = str(path).encode("utf-8") if sys.platform == "win32" else os.fsencode(path)
            path_buffer = c.create_string_buffer(path_bytes)
            signature = [ptr, size, c.POINTER(ptr)]
            call("file_create", signature, path_buffer, len(path_bytes), c.byref(db))
            other = ptr()
            call("file_open", signature, path_buffer, len(path_bytes), c.byref(other), status=6)
            assert other.value is None
            call("close", [ptr], db)
            before, after = u64(1), u64(1)
            call("file_compact", [ptr, size, c.POINTER(u64), c.POINTER(u64)],
                 path_buffer, len(path_bytes), c.byref(before), c.byref(after),
                 status=0 if can_replace else 20)
            if can_replace:
                assert before.value >= after.value > 0
            else:
                assert before.value == after.value == 0
                invalid = c.create_string_buffer(b"\xff")
                call("file_open", signature, invalid, 1, c.byref(db), status=1)
                assert db.value is None
            call("file_open", signature, path_buffer, len(path_bytes), c.byref(db))
            call("close", [ptr], db)
    print("Shared-library C ABI smoke passed:", sys.argv[1])


if __name__ == "__main__":
    main()
