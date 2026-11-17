package main

import "core:c"
import "core:crypto/sha2"
import hexadecimal "core:encoding/hex"
import "core:encoding/json"
import "core:os"
import win "core:sys/windows"
import db "weekend:odin"

foreign import kernel "system:Kernel32.lib"
foreign kernel {
    CompareStringOrdinal :: proc "system"(left: win.LPCWSTR, left_len: c.int,
                                           right: win.LPCWSTR, right_len: c.int,
                                           ignore_case: win.BOOL) -> c.int ---
    SetErrorMode :: proc "system"(mode: win.UINT) -> win.UINT ---
}

run_with_attested_dll :: proc(body: proc()) {
    // Turn critical, fault and file-open dialogs into ordinary process errors.
    SetErrorMode(0x0001 | 0x0002 | 0x8000)
    assert(len(os.args) >= 8)
    module := win.GetModuleHandleW(win.utf8_to_wstring("weekend_db.dll"))
    assert(module != nil && win.GetProcAddress(module, "wdb_source_id") != nil)
    loaded, expected: [32768]u16
    loaded_len := win.GetModuleFileNameW(module, &loaded[0], len(loaded))
    expected_path := win.utf8_to_wstring(os.args[4])
    assert(expected_path != nil)
    expected_len := win.GetFullPathNameW(expected_path, len(expected), win.LPCWSTR(&expected[0]), nil)
    assert(loaded_len > 0 && loaded_len < len(loaded))
    assert(expected_len > 0 && expected_len < len(expected))
    CSTR_EQUAL :: 2
    assert(CompareStringOrdinal(win.LPCWSTR(&loaded[0]), c.int(loaded_len),
                                win.LPCWSTR(&expected[0]), c.int(expected_len), true) == CSTR_EQUAL)

    // Hash and hold the loaded file with writes and deletion denied through the trace.
    dll := win.CreateFileW(win.LPCWSTR(&loaded[0]), win.GENERIC_READ, win.FILE_SHARE_READ,
                           nil, win.OPEN_EXISTING, win.FILE_ATTRIBUTE_NORMAL, nil)
    assert(dll != win.INVALID_HANDLE_VALUE)
    defer assert(bool(win.CloseHandle(dll)))
    loaded_path, conversion_error := win.utf16_to_utf8(loaded[:int(loaded_len)])
    assert(conversion_error == nil)
    attest_dll(dll, loaded_path, "before", os.args[6])
    body()
    attest_dll(dll, loaded_path, "after", os.args[7])
}

attest_dll :: proc(dll: win.HANDLE, loaded_path, phase, proof_path: string) {
    size: win.LARGE_INTEGER
    assert(bool(win.GetFileSizeEx(dll, &size)) && size > 0 && size <= 256 * 1024 * 1024)
    assert(bool(win.SetFilePointerEx(dll, 0, nil, win.FILE_BEGIN)))
    ctx: sha2.Context_256
    sha2.init_256(&ctx)
    buffer: [65536]byte
    total: i64
    for {
        count: win.DWORD
        assert(bool(win.ReadFile(dll, &buffer, len(buffer), &count, nil)))
        if count == 0 { break }
        total += i64(count)
        assert(total <= i64(size))
        sha2.update(&ctx, buffer[:int(count)])
    }
    assert(total == i64(size))
    digest: [sha2.DIGEST_SIZE_256]byte
    sha2.final(&ctx, digest[:])
    hash, encode_error := hexadecimal.encode(digest[:])
    assert(encode_error == nil)
    defer delete(hash)
    assert(string(hash) == os.args[5])

    identity: [256]u8
    length: c.size_t
    ok(db.wdb_source_id(raw_data(identity[:]), len(identity), &length))
    assert(length <= len(identity))
    source := string(identity[:int(length)])
    assert(source == os.args[3])
    abi, data_format := db.wdb_abi_version(), db.wdb_format_version()
    assert(abi == db.WDB_ABI_VERSION && data_format == db.WDB_FORMAT_VERSION)
    record := struct {
        schema, pid: u32,
        phase, loaded_dll, sha256, source_id: string,
        abi, format, features: u32,
    }{1, u32(win.GetCurrentProcessId()), phase, loaded_path, string(hash), source,
      abi, data_format, db.wdb_features()}
    bytes, marshal_error := json.marshal(record)
    assert(marshal_error == nil)
    defer delete(bytes)
    file, open_error := os.open(proof_path, {.Write, .Create, .Excl})
    assert(open_error == nil)
    written, write_error := os.write(file, bytes)
    assert(write_error == nil && written == len(bytes))
    assert(os.close(file) == nil)
}
