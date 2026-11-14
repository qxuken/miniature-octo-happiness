package main
import "core:c"
import "core:fmt"
import "core:os"
import db "weekend:odin"

put :: proc(database: db.Database, collection, number: u64) {
    value: db.Value
    assert(db.wdb_value_make(database, db.WDB_UNSIGNED, number, nil, 0, &value) == db.WDB_OK)
    defer assert(db.wdb_value_release(value) == db.WDB_OK)
    tx: db.Transaction
    assert(db.wdb_tx_begin(database, db.WDB_RECORDS, &tx) == db.WDB_OK)
    defer assert(db.wdb_tx_release(tx) == db.WDB_OK)
    key := "row"
    assert(db.wdb_tx_put(tx, collection, raw_data(key), c.size_t(len(key)), value) == db.WDB_OK)
    revision: u64
    assert(db.wdb_tx_commit(tx, &revision) == db.WDB_OK)
}
check :: proc(snapshot: db.Snapshot, collection, expected: u64) {
    key := "row"
    value: db.Value
    assert(db.wdb_snapshot_get(snapshot, collection, raw_data(key), c.size_t(len(key)), &value) == db.WDB_OK)
    defer assert(db.wdb_value_release(value) == db.WDB_OK)
    number: u64
    assert(db.wdb_value_scalar(value, &number) == db.WDB_OK && number == expected)
}
memory :: proc() {
    database: db.Database
    assert(db.wdb_open(&database) == db.WDB_OK)
    defer assert(db.wdb_close(database) == db.WDB_OK)
    bytes := [3]u8{65, 0, 66}
    value: db.Value
    assert(db.wdb_value_make(database, db.WDB_BYTES, 0, raw_data(bytes[:]), 3, &value) == db.WDB_OK)
    defer assert(db.wdb_value_release(value) == db.WDB_OK)
    copied: [3]u8
    required: c.size_t
    assert(db.wdb_value_bytes(value, raw_data(copied[:]), 2, &required) == db.WDB_BUFFER && required == 3)
    assert(db.wdb_value_bytes(value, raw_data(copied[:]), 3, &required) == db.WDB_OK && copied == bytes)
    collection: u64
    assert(db.wdb_collection_create(database, &collection) == db.WDB_OK)
    put(database, collection, 17)
    snapshot: db.Snapshot
    assert(db.wdb_snapshot_acquire(database, &snapshot) == db.WDB_OK)
    defer assert(db.wdb_snapshot_release(snapshot) == db.WDB_OK)
    check(snapshot, collection, 17)
}
file :: proc(path: string) {
    database: db.Database
    assert(db.wdb_file_create(raw_data(path), c.size_t(len(path)), &database) == db.WDB_OK)
    collection, retained: u64
    assert(db.wdb_collection_create(database, &collection) == db.WDB_OK)
    put(database, collection, 17)
    old: db.Snapshot
    assert(db.wdb_snapshot_acquire(database, &old) == db.WDB_OK)
    assert(db.wdb_snapshot_retain(database, old, &retained) == db.WDB_OK)
    assert(db.wdb_snapshot_release(old) == db.WDB_OK)
    put(database, collection, 18)
    assert(db.wdb_close(database) == db.WDB_OK)
    assert(db.wdb_file_open(raw_data(path), c.size_t(len(path)), &database) == db.WDB_OK)
    current: db.Snapshot
    assert(db.wdb_snapshot_acquire(database, &current) == db.WDB_OK)
    check(current, collection, 18)
    assert(db.wdb_snapshot_release(current) == db.WDB_OK)
    assert(db.wdb_retained_open(database, retained, &old) == db.WDB_OK)
    check(old, collection, 17)
    assert(db.wdb_retained_release(database, retained) == db.WDB_OK)
    assert(db.wdb_snapshot_release(old) == db.WDB_OK)
    assert(db.wdb_close(database) == db.WDB_OK)
}
main :: proc() {
    assert(len(os.args) == 3)
    assert(db.wdb_abi_version() == db.WDB_ABI_VERSION)
    assert(db.wdb_format_version() == db.WDB_FORMAT_VERSION)
    source: [256]u8
    required: c.size_t
    assert(db.wdb_source_id(raw_data(source[:]), 256, &required) == db.WDB_OK)
    assert(string(source[:int(required)]) == os.args[2])
    memory()
    file(os.args[1])
    fmt.println("Packaged Odin ABI identity, memory, transaction and retained-file reopen: passed")
}
