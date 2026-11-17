package main

import "core:c"
import "core:dynlib"
import "core:fmt"
import "core:os"
import db "weekend:odin"

#assert(!ODIN_DISABLE_ASSERT)
REQUEST_KEY :: string("request")

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

Fault_Hooks :: struct {
    arm: proc "c" (cstring, c.int),
    disarm: proc "c" (),
    count: proc "c" (c.int) -> c.int,
}

fault_symbol :: proc(shim: dynlib.Library, name: string) -> rawptr {
    symbol, found := dynlib.symbol_address(shim, name)
    assert(found)
    return symbol
}

bind_fault_hooks :: proc(shim: dynlib.Library) -> Fault_Hooks {
    return {
        cast(proc "c" (cstring, c.int))fault_symbol(shim, "qualification_arm"),
        cast(proc "c" ())fault_symbol(shim, "qualification_disarm"),
        cast(proc "c" (c.int) -> c.int)fault_symbol(shim, "qualification_count"),
    }
}

verify_commit :: proc(database: db.Database, candidate_tx: db.Transaction, mode: int, hooks: Fault_Hooks) {
    assert(hooks.count(0) == 0 && hooks.count(1) == 0)
    expected := [7]db.Status{db.WDB_OK, db.WDB_IO, db.WDB_OUTCOME_UNKNOWN,
                             db.WDB_OUTCOME_UNKNOWN, db.WDB_OUTCOME_UNKNOWN, db.WDB_IO, db.WDB_IO}
    tx := candidate_tx
    revision: u64 = 0xdead_beef_cafe_babe
    status := db.wdb_tx_commit(tx, &revision)
    if status != expected[mode] {
        fmt.eprintf("unexpected ABI commit status for mode %d: got %d, expected %d\n", mode, status, expected[mode])
        os.exit(65)
    }
    assert(revision == (4 if mode == 0 else 0))
    hits, syncs, roots, witnesses, native_error := hooks.count(0), hooks.count(1), hooks.count(2), hooks.count(3), hooks.count(5)
    assert(hooks.count(4) > 0)
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
    hooks.disarm()
    ok(db.wdb_close(database))
    fmt.printf("%c\"mode\":%d,\"status\":%d,\"visible\":%d,\"hits\":%d,\"syncs\":%d,\"root_writes\":%d,\"witness_writes\":%d,\"native_errno\":%d,\"verified\":true%c\n",
               '{', mode, status, visible, hits, syncs, roots, witnesses, native_error, '}')
}

recover :: proc(path: string, mode: int, verify_only := false) {
    database: db.Database
    ok(db.wdb_file_open(raw_data(path), c.size_t(len(path)), &database))
    reopened: u64 = 4 if mode == 0 || mode == 2 || mode == 3 else 3
    published := verify(database, reopened)
    if verify_only {
        assert((mode == 5 || mode == 6) && !published)
        ok(db.wdb_close(database))
        fmt.printf("%c\"mode\":%d,\"reopened\":3,\"retained_revision\":2,\"verified\":true%c\n", '{', mode, '}')
        return
    }
    if !published {
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

verify_identity :: proc(source: string) {
    identity: [64]u8
    required: c.size_t
    ok(db.wdb_source_id(raw_data(identity[:]), len(identity), &required))
    assert(required <= len(identity) && string(identity[:int(required)]) == source)
    assert(db.wdb_abi_version() == db.WDB_ABI_VERSION &&
           db.wdb_format_version() == db.WDB_FORMAT_VERSION && db.wdb_features() == 3)
}
