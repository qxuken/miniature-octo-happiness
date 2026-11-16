#+build linux, darwin

package main

import "core:c"
import "core:dynlib"
import "core:fmt"
import "core:os"
import "core:strconv"
import "core:strings"
import "core:sys/posix"

when ODIN_OS == .Linux {
    MODE_COUNT :: 6
    PRELOAD :: "LD_PRELOAD"
} else {
    MODE_COUNT :: 5
    PRELOAD :: "DYLD_INSERT_LIBRARIES"
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
    hooks := bind_fault_hooks(shim)

    database := seed(path)
    if mode == 5 {
        fill := cast(proc "c" (cstring))fault_symbol(shim, "qualification_fill_tmpfs")
        fill(strings.clone_to_cstring(directory, context.temp_allocator))
    }
    tx := candidate(database)
    hooks.arm(strings.clone_to_cstring(path, context.temp_allocator), c.int(0 if mode == 5 else mode))
    if mode != 5 {
        probe_unrelated(directory)
    }
    verify_commit(database, tx, mode, hooks)
    if mode == 5 {
        ballast := strings.clone_to_cstring(fmt.tprintf("%s/ballast", directory), context.temp_allocator)
        assert(posix.unlink(ballast) == .OK)
    }
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
    verify_identity(os.args[3])

    path := fmt.tprintf("%s/database.wdb", os.args[2])
    if recovering {
        recover(path, mode)
    } else {
        exercise(os.args[2], path, mode)
    }
}
