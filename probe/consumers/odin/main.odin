package main

import "core:fmt"
import "core:c"

PROBE_LIB :: #config(PROBE_LIB, "../../build/c-debug/libweekend_probe.a")
foreign import probe { PROBE_LIB }

Db :: distinct rawptr
Value :: distinct rawptr
foreign probe {
    wp_open :: proc(out: ^Db) -> i32 ---
    wp_close :: proc(db: Db) -> i32 ---
    wp_make :: proc(db: Db, kind: i32, bytes: [^]u8, len: c.size_t, out: ^Value) -> i32 ---
    wp_clone :: proc(value: Value, out: ^Value) -> i32 ---
    wp_release :: proc(value: Value) ---
    wp_map_set :: proc(dict: Value, key: [^]u8, len: c.size_t, value: Value, out: ^Value) -> i32 ---
    wp_map_get :: proc(dict: Value, key: [^]u8, len: c.size_t, out: ^Value) -> i32 ---
    wp_list_push :: proc(list: Value, value: Value, out: ^Value) -> i32 ---
    wp_list_get :: proc(list: Value, index: c.size_t, out: ^Value) -> i32 ---
    wp_read_bytes :: proc(value: Value, buffer: [^]u8, capacity: c.size_t, required: ^c.size_t) -> i32 ---
}

main :: proc() {
    db: Db
    assert(wp_open(&db) == 0)
    // Defers run in reverse order: values are released before close.
    defer assert(wp_close(db) == 0)
    bytes, list, dict, tags, old, clone, more, edited: Value
    input := [3]u8{'a', 0, 'b'}
    assert(wp_make(db, 1, raw_data(input[:]), 3, &bytes) == 0)
    defer wp_release(bytes)
    assert(wp_make(db, 3, nil, 0, &list) == 0)
    defer wp_release(list)
    assert(wp_make(db, 2, nil, 0, &dict) == 0)
    defer wp_release(dict)
    assert(wp_list_push(list, bytes, &tags) == 0)
    defer wp_release(tags)
    key := "tags"
    assert(wp_map_set(dict, raw_data(key), 4, tags, &old) == 0)
    defer wp_release(old)
    assert(wp_clone(old, &clone) == 0)
    defer wp_release(clone)
    assert(wp_list_push(tags, bytes, &more) == 0)
    defer wp_release(more)
    assert(wp_map_set(clone, raw_data(key), 4, more, &edited) == 0)
    defer wp_release(edited)
    assert(wp_close(db) == 6)

    got, item, bad: Value
    assert(wp_map_get(clone, raw_data(key), 4, &got) == 0)
    assert(wp_list_get(got, 1, &bad) == 4 && bad == nil)
    assert(wp_list_get(got, 0, &item) == 0)
    wp_release(got)
    buffer: [3]u8
    required: c.size_t
    assert(wp_read_bytes(item, raw_data(buffer[:]), 3, &required) == 0)
    assert(required == 3 && buffer == input)
    wp_release(item)
    assert(wp_map_get(edited, raw_data(key), 4, &got) == 0)
    assert(wp_list_get(got, 1, &item) == 0)
    wp_release(got)
    wp_release(item)
    check_integration()
    fmt.println("Odin consumer: ok")
}
