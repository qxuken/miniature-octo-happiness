package main
import "core:fmt"
import "core:c"
import wgpu "vendor:wgpu"

ENGINE_LIB :: #config(ENGINE_LIB, "../../target/debug/libweekend_db.dylib")
foreign import engine { ENGINE_LIB }
Db :: distinct rawptr
Value :: distinct rawptr
Tx :: distinct rawptr
Snapshot :: distinct rawptr
foreign engine {
    wdb_open :: proc(out: ^Db) -> i32 ---
    wdb_close :: proc(db: Db) -> i32 ---
    wdb_collection_create :: proc(db: Db, out: ^u64) -> i32 ---
    wdb_value_make :: proc(db: Db, kind: i32, bits: u64, bytes: [^]u8, len: c.size_t, out: ^Value) -> i32 ---
    wdb_value_release :: proc(value: Value) -> i32 ---
    wdb_value_scalar :: proc(value: Value, out: ^u64) -> i32 ---
    wdb_map_set :: proc(dict: Value, key: [^]u8, len: c.size_t, value: Value, out: ^Value) -> i32 ---
    wdb_map_get :: proc(dict: Value, key: [^]u8, len: c.size_t, out: ^Value) -> i32 ---
    wdb_tx_begin :: proc(db: Db, validation: i32, out: ^Tx) -> i32 ---
    wdb_tx_put :: proc(tx: Tx, collection: u64, key: [^]u8, len: c.size_t, value: Value) -> i32 ---
    wdb_tx_commit :: proc(tx: Tx, out: ^u64) -> i32 ---
    wdb_tx_release :: proc(tx: Tx) -> i32 ---
    wdb_snapshot_acquire :: proc(db: Db, out: ^Snapshot) -> i32 ---
    wdb_snapshot_get :: proc(snapshot: Snapshot, collection: u64, key: [^]u8, len: c.size_t, out: ^Value) -> i32 ---
    wdb_snapshot_release :: proc(snapshot: Snapshot) -> i32 ---
}
main :: proc() {
    db: Db
    assert(wdb_open(&db) == 0)
    defer assert(wdb_close(db) == 0)
    collection, revision: u64
    assert(wdb_collection_create(db, &collection) == 0)
    zero, one, dict, record: Value
    assert(wdb_value_make(db, 2, 0, nil, 0, &zero) == 0)
    defer assert(wdb_value_release(zero) == 0)
    assert(wdb_value_make(db, 2, 1, nil, 0, &one) == 0)
    defer assert(wdb_value_release(one) == 0)
    assert(wdb_value_make(db, 6, 0, nil, 0, &dict) == 0)
    defer assert(wdb_value_release(dict) == 0)
    field := "count"
    assert(wdb_map_set(dict, raw_data(field), 5, zero, &record) == 0)
    defer assert(wdb_value_release(record) == 0)
    seed: Tx
    assert(wdb_tx_begin(db, 0, &seed) == 0)
    a, b := "a", "b"
    assert(wdb_tx_put(seed, collection, raw_data(a), 1, record) == 0)
    assert(wdb_tx_put(seed, collection, raw_data(b), 1, zero) == 0)
    assert(wdb_tx_commit(seed, &revision) == 0)
    assert(wdb_tx_release(seed) == 0)
    old: Snapshot
    assert(wdb_snapshot_acquire(db, &old) == 0)
    defer assert(wdb_snapshot_release(old) == 0)
    first, second: Tx
    assert(wdb_tx_begin(db, 0, &first) == 0)
    defer assert(wdb_tx_release(first) == 0)
    assert(wdb_tx_begin(db, 0, &second) == 0)
    defer assert(wdb_tx_release(second) == 0)
    changed: Value
    assert(wdb_map_set(record, raw_data(field), 5, one, &changed) == 0)
    defer assert(wdb_value_release(changed) == 0)
    assert(wdb_tx_put(first, collection, raw_data(a), 1, changed) == 0)
    assert(wdb_tx_put(second, collection, raw_data(b), 1, one) == 0)
    assert(wdb_tx_commit(first, &revision) == 0)
    assert(wdb_tx_commit(second, &revision) == 0)
    assert(wdb_close(db) == 6)
    old_record, old_count: Value
    assert(wdb_snapshot_get(old, collection, raw_data(a), 1, &old_record) == 0)
    defer assert(wdb_value_release(old_record) == 0)
    assert(wdb_map_get(old_record, raw_data(field), 5, &old_count) == 0)
    defer assert(wdb_value_release(old_count) == 0)
    count: u64
    assert(wdb_value_scalar(old_count, &count) == 0 && count == 0)
    version := wgpu.GetVersion()
    assert(version != 0)
    fmt.printf("Odin + Rust shared engine + static wgpu-native: passed (wgpu %x)\n", version)
}
