package main

import "core:c"
import "core:dynlib"
import "core:fmt"
import "core:os"
import "core:strconv"
import "core:strings"
import "core:sys/posix"
import db "weekend:odin"

#assert(ODIN_OS == .Darwin || ODIN_OS == .Linux)
#assert(!ODIN_DISABLE_ASSERT)
REQUEST_KEY :: string("request")
when ODIN_OS == .Linux {
    MODE_COUNT :: 6
    PRELOAD :: "LD_PRELOAD"
} else {
    MODE_COUNT :: 5
    PRELOAD :: "DYLD_INSERT_LIBRARIES"
}

ok :: proc(status: db.Status) {
    assert(status == db.WDB_OK, fmt.tprintf("unexpected status %d", status))
}

put_blob :: proc(database: db.Database, tx: db.Transaction, key: string, length: int, byte: u8) {
    buffer: [65536]u8
    assert(length <= len(buffer))
    for &item in buffer[:length] {
        item = byte
    }
    value: db.Value
    ok(db.wdb_value_make(database, db.WDB_BYTES, 0, raw_data(buffer[:]), c.size_t(length), &value))
    ok(db.wdb_tx_put(tx, 1, raw_data(key), c.size_t(len(key)), value))
    ok(db.wdb_value_release(value))
}

verify_blob :: proc(view: db.Snapshot, key: string, length: int, byte: u8) {
    value: db.Value
    ok(db.wdb_snapshot_get(view, 1, raw_data(key), c.size_t(len(key)), &value))
    buffer: [65536]u8
    required: c.size_t
    ok(db.wdb_value_bytes(value, raw_data(buffer[:]), len(buffer), &required))
    assert(required == c.size_t(length))
    for item in buffer[:length] {
        assert(item == byte)
    }
    ok(db.wdb_value_release(value))
}

verify_view :: proc(view: db.Snapshot, revision: u64) -> bool {
    seen: u64
    ok(db.wdb_snapshot_revision(view, &seen))
    assert(seen == revision)
    expected_keys := []string{"row"}
    if revision == 4 {
        expected_keys = []string{"payload", "request", "row"}
    }
    scan: db.Scan
    count: c.size_t
    ok(db.wdb_snapshot_scan(view, 1, &scan))
    ok(db.wdb_scan_len(scan, &count))
    assert(count == c.size_t(len(expected_keys)))
    for key, index in expected_keys {
        buffer: [32]u8
        required: c.size_t
        ok(db.wdb_scan_key(scan, c.size_t(index), raw_data(buffer[:]), len(buffer), &required))
        assert(required <= len(buffer) && string(buffer[:int(required)]) == key)
    }
    ok(db.wdb_scan_release(scan))

    marker: db.Value
    marker_status := db.wdb_snapshot_get(view, 1, raw_data(REQUEST_KEY), len(REQUEST_KEY), &marker)
    published := marker_status == db.WDB_OK
    assert(published == (revision == 4))
    if published {
        kind: i32
        ok(db.wdb_value_kind(marker, &kind))
        assert(kind == db.WDB_UNSIGNED)
        request_id: u64
        ok(db.wdb_value_scalar(marker, &request_id))
        assert(request_id == 42)
        ok(db.wdb_value_release(marker))
        verify_blob(view, "payload", 65536, 0xa5)
        verify_blob(view, "row", 32768, 2)
    } else {
        assert(marker_status == db.WDB_NOT_FOUND && marker == nil)
        verify_blob(view, "row", 32768, 1)
    }
    return published
}

verify :: proc(database: db.Database, revision: u64) -> bool {
    current, retained: db.Snapshot
    ok(db.wdb_snapshot_acquire(database, &current))
    published := verify_view(current, revision)
    ok(db.wdb_snapshot_release(current))

    ids: [2]u64
    count: c.size_t
    ok(db.wdb_retained_ids(database, raw_data(ids[:]), len(ids), &count))
    assert(count == 1 && ids[0] == 1)
    ok(db.wdb_retained_open(database, ids[0], &retained))
    assert(!verify_view(retained, 2))
    ok(db.wdb_snapshot_release(retained))
    return published
}

seed :: proc(path: string) -> db.Database {
    database: db.Database
    ok(db.wdb_file_create(raw_data(path), c.size_t(len(path)), &database))
    collection: u64
    ok(db.wdb_collection_create(database, &collection))
    assert(collection == 1)
    tx: db.Transaction
    ok(db.wdb_tx_begin(database, db.WDB_RECORDS, &tx))
    put_blob(database, tx, "row", 32768, 1)
    revision, id: u64
    ok(db.wdb_tx_commit(tx, &revision))
    assert(revision == 2)
    ok(db.wdb_tx_release(tx))
    view: db.Snapshot
    ok(db.wdb_snapshot_acquire(database, &view))
    ok(db.wdb_snapshot_retain(database, view, &id))
    assert(id == 1)
    ok(db.wdb_snapshot_release(view))
    assert(!verify(database, 3))
    return database
}

candidate :: proc(database: db.Database) -> db.Transaction {
    tx: db.Transaction
    ok(db.wdb_tx_begin(database, db.WDB_RECORDS, &tx))
    put_blob(database, tx, "row", 32768, 2)
    put_blob(database, tx, "payload", 65536, 0xa5)
    marker: db.Value
    ok(db.wdb_value_make(database, db.WDB_UNSIGNED, 42, nil, 0, &marker))
    ok(db.wdb_tx_put(tx, 1, raw_data(REQUEST_KEY), len(REQUEST_KEY), marker))
    ok(db.wdb_value_release(marker))
    return tx
}

probe_unrelated :: proc(directory: string) {
    path := strings.clone_to_cstring(fmt.tprintf("%s/unrelated", directory), context.temp_allocator)
    fd := posix.open(path, {.WRONLY, .CREAT, .EXCL}, {.IRUSR, .IWUSR})
    assert(fd != -1)
    when ODIN_OS == .Darwin {
        // Darwin F_FULLFSYNC from <sys/fcntl.h>. Odin's POSIX enum omits it.
        full_fsync := transmute(posix.FCNTL_Cmd)c.int(51)
        assert(posix.fcntl(fd, full_fsync) == 0)
    } else {
        assert(posix.fsync(fd) == .OK)
    }
    assert(posix.close(fd) == .OK)
}

exercise :: proc(directory, path: string, mode: int) {
    shim_path, present := os.lookup_env(PRELOAD, context.temp_allocator)
    assert(present)
    shim, loaded := dynlib.load_library(shim_path)
    assert(loaded)
    defer assert(dynlib.unload_library(shim))
    arm_address, has_arm := dynlib.symbol_address(shim, "qualification_arm")
    disarm_address, has_disarm := dynlib.symbol_address(shim, "qualification_disarm")
    count_address, has_count := dynlib.symbol_address(shim, "qualification_count")
    assert(has_arm && has_disarm && has_count)
    arm := cast(proc "c" (cstring, c.int))arm_address
    disarm := cast(proc "c" ())disarm_address
    count := cast(proc "c" (c.int) -> c.int)count_address

    database := seed(path)
    if mode == 5 {
        fill_address, has_fill := dynlib.symbol_address(shim, "qualification_fill_tmpfs")
        assert(has_fill)
        fill := cast(proc "c" (cstring))fill_address
        fill(strings.clone_to_cstring(directory, context.temp_allocator))
    }
    tx := candidate(database)
    arm(strings.clone_to_cstring(path, context.temp_allocator), c.int(0 if mode == 5 else mode))
    if mode != 5 {
        probe_unrelated(directory)
    }
    assert(count(0) == 0 && count(1) == 0)

    expected := [6]db.Status{db.WDB_OK, db.WDB_IO, db.WDB_OUTCOME_UNKNOWN,
                             db.WDB_OUTCOME_UNKNOWN, db.WDB_OUTCOME_UNKNOWN, db.WDB_IO}
    revision: u64 = 0xdead_beef_cafe_babe
    status := db.wdb_tx_commit(tx, &revision)
    if status != expected[mode] {
        fmt.eprintf("unexpected ABI commit status for mode %d: got %d, expected %d\n", mode, status, expected[mode])
        os.exit(65)
    }
    assert(revision == (4 if mode == 0 else 0))
    hits, syncs, roots, witnesses, native_error := count(0), count(1), count(2), count(3), count(5)
    assert(count(4) > 0)
    visible: u64 = 4 if mode == 0 else 3
    verify(database, visible)
    revision = 99
    assert(db.wdb_tx_commit(tx, &revision) == db.WDB_CLOSED && revision == 0)
    ok(db.wdb_tx_release(tx))
    if mode != 0 {
        ok(db.wdb_tx_begin(database, db.WDB_RECORDS, &tx))
        revision = 99
        assert(db.wdb_tx_commit(tx, &revision) == db.WDB_RECOVERY_REQUIRED && revision == 0)
        revision = 99
        assert(db.wdb_tx_commit(tx, &revision) == db.WDB_CLOSED && revision == 0)
        ok(db.wdb_tx_release(tx))
    }
    disarm()
    ok(db.wdb_close(database))
    if mode == 5 {
        ballast := strings.clone_to_cstring(fmt.tprintf("%s/ballast", directory), context.temp_allocator)
        assert(posix.unlink(ballast) == .OK)
    }
    fmt.printf("%c\"mode\":%d,\"status\":%d,\"visible\":%d,\"hits\":%d,\"syncs\":%d,\"root_writes\":%d,\"witness_writes\":%d,\"native_errno\":%d,\"verified\":true%c\n",
               '{', mode, status, visible, hits, syncs, roots, witnesses, native_error, '}')
}

recover :: proc(path: string, mode: int) {
    database: db.Database
    ok(db.wdb_file_open(raw_data(path), c.size_t(len(path)), &database))
    reopened: u64 = 4 if mode == 0 || mode == 2 || mode == 3 else 3
    if !verify(database, reopened) {
        tx := candidate(database)
        revision: u64
        ok(db.wdb_tx_commit(tx, &revision))
        assert(revision == 4)
        ok(db.wdb_tx_release(tx))
    }
    assert(verify(database, 4))
    ok(db.wdb_close(database))
    ok(db.wdb_file_open(raw_data(path), c.size_t(len(path)), &database))
    assert(verify(database, 4))
    ok(db.wdb_close(database))
    fmt.printf("%c\"mode\":%d,\"reopened\":%d,\"final\":4,\"retained_revision\":2,\"verified\":true%c\n", '{', mode, reopened, '}')
}

main :: proc() {
    assert(len(os.args) == 4 || len(os.args) == 5)
    no_core: posix.rlimit
    assert(posix.setrlimit(.CORE, &no_core) == .OK)
    recovering := len(os.args) == 5
    if recovering {
        assert(os.args[1] == "recover")
    }
    mode, valid := strconv.parse_int(os.args[4] if recovering else os.args[1])
    assert(valid && mode >= 0 && mode < MODE_COUNT)

    identity: [64]u8
    required: c.size_t
    ok(db.wdb_source_id(raw_data(identity[:]), len(identity), &required))
    assert(required <= len(identity) && string(identity[:int(required)]) == os.args[3])
    assert(db.wdb_abi_version() == db.WDB_ABI_VERSION &&
           db.wdb_format_version() == db.WDB_FORMAT_VERSION && db.wdb_features() == 3)

    path := fmt.tprintf("%s/database.wdb", os.args[2])
    if recovering {
        recover(path, mode)
    } else {
        exercise(os.args[2], path, mode)
    }
}
