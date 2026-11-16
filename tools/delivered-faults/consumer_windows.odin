package main

import "core:c"
import "core:dynlib"
import "core:fmt"
import "core:os"
import "core:strconv"
import "core:strings"
import win "core:sys/windows"

main :: proc() {
    assert(len(os.args) == 8 || len(os.args) == 9)
    run_with_attested_dll(run_fault_case)
}

run_fault_case :: proc() {
    verify_identity(os.args[3])
    directory := os.args[2]
    path := fmt.tprintf("%s/database.wdb", directory)
    action := os.args[1]
    if len(os.args) == 8 {
        assert(win.GetModuleHandleW(win.utf8_to_wstring("fault-shim.dll")) == nil)
        assert(win.GetModuleHandleW(win.utf8_to_wstring("fault-disabled.dll")) == nil)
        if action == "verify-full" {
            recover(path, 6, verify_only = true)
        } else {
            assert(strings.has_prefix(action, "recover-"))
            mode, valid := strconv.parse_int(action[len("recover-"):])
            assert(valid && mode >= 0 && mode <= 6)
            recover(path, mode)
        }
        return
    }
    mode, valid := strconv.parse_int(action)
    assert(valid && mode >= 0 && mode <= 6)
    shim, loaded := dynlib.load_library(os.args[8])
    assert(loaded)
    defer assert(dynlib.unload_library(shim))
    hooks := bind_fault_hooks(shim)
    probe := cast(proc "c" (cstring))fault_symbol(shim, "qualification_probe_unrelated")

    database := seed(path)
    tx := candidate(database)
    hooks.arm(strings.clone_to_cstring(path, context.temp_allocator), c.int(mode))
    probe(strings.clone_to_cstring(fmt.tprintf("%s/unrelated", directory), context.temp_allocator))
    if mode == 6 {
        fill := cast(proc "c" (cstring))fault_symbol(shim, "qualification_fill_ntfs")
        fill(strings.clone_to_cstring(directory, context.temp_allocator))
    }
    verify_commit(database, tx, mode, hooks)
}
