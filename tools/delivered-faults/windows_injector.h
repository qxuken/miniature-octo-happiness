/* Private x64 PE import interception in one single-threaded fixture process. */
#include <stddef.h>
#include <winternl.h>

typedef BOOL (WINAPI *flush_fn)(HANDLE);
typedef NTSTATUS (NTAPI *write_fn)(HANDLE, HANDLE, PIO_APC_ROUTINE, PVOID,
                                  PIO_STATUS_BLOCK, const void *, ULONG,
                                  const LARGE_INTEGER *, const ULONG *);

struct import_patch {
    ULONG_PTR *slot;
    ULONG_PTR original;
    ULONG_PTR replacement;
};

static flush_fn real_flush;
static write_fn real_write;
static struct import_patch flush_patch, write_patch;
static wchar_t target_path[32768];
static int selected, hits, syncs, roots, witnesses, writes, win_error;

static void *image_at(BYTE *base, DWORD size, ULONGLONG offset, size_t bytes) {
    assert(offset <= size && bytes <= size - offset);
    return base + (size_t)offset;
}

static struct import_patch find_import(HMODULE module, const char *name,
                                       ULONG_PTR replacement) {
    BYTE *base = (BYTE *)module;
    const IMAGE_DOS_HEADER *dos = (const IMAGE_DOS_HEADER *)base;
    assert(dos->e_magic == IMAGE_DOS_SIGNATURE && dos->e_lfanew > 0 && dos->e_lfanew < 4096);
    const IMAGE_NT_HEADERS64 *nt = (const IMAGE_NT_HEADERS64 *)(base + dos->e_lfanew);
    assert(nt->Signature == IMAGE_NT_SIGNATURE && nt->FileHeader.Machine == IMAGE_FILE_MACHINE_AMD64);
    assert(nt->OptionalHeader.Magic == IMAGE_NT_OPTIONAL_HDR64_MAGIC);
    assert(nt->OptionalHeader.NumberOfRvaAndSizes > IMAGE_DIRECTORY_ENTRY_IMPORT);
    DWORD size = nt->OptionalHeader.SizeOfImage;
    IMAGE_DATA_DIRECTORY dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    assert(dir.VirtualAddress > 0 && dir.Size >= sizeof(IMAGE_IMPORT_DESCRIPTOR));
    const IMAGE_IMPORT_DESCRIPTOR *imports = (const IMAGE_IMPORT_DESCRIPTOR *)
        image_at(base, size, dir.VirtualAddress, dir.Size);
    struct import_patch found = {0};
    for (size_t i = 0; i < dir.Size / sizeof(*imports) && imports[i].Name; ++i) {
        assert(imports[i].OriginalFirstThunk && imports[i].FirstThunk);
        for (size_t entry = 0;; ++entry) {
            ULONGLONG displacement = (ULONGLONG)entry * sizeof(IMAGE_THUNK_DATA64);
            const IMAGE_THUNK_DATA64 *names = (const IMAGE_THUNK_DATA64 *)
                image_at(base, size, imports[i].OriginalFirstThunk + displacement, sizeof(*names));
            ULONGLONG offset = names->u1.AddressOfData;
            if (!offset) break;
            if (IMAGE_SNAP_BY_ORDINAL64(offset)) continue;
            offset += offsetof(IMAGE_IMPORT_BY_NAME, Name);
            const char *import_name = (const char *)image_at(base, size, offset, 1);
            assert(memchr(import_name, '\0', size - (size_t)offset) != NULL);
            if (strcmp(import_name, name) != 0) continue;
            assert(found.slot == NULL);
            IMAGE_THUNK_DATA64 *thunk = (IMAGE_THUNK_DATA64 *)
                image_at(base, size, imports[i].FirstThunk + displacement, sizeof(*thunk));
            found.slot = &thunk->u1.Function;
            found.original = *found.slot;
            found.replacement = replacement;
        }
    }
    assert(found.slot != NULL && found.original != 0 && found.original != replacement);
    return found;
}

static void patch(struct import_patch entry, int install) {
    DWORD previous, ignored;
    ULONG_PTR expected = install ? entry.original : entry.replacement;
    assert(*entry.slot == expected);
    assert(VirtualProtect(entry.slot, sizeof(*entry.slot), PAGE_READWRITE, &previous));
    *entry.slot = install ? entry.replacement : entry.original;
    assert(VirtualProtect(entry.slot, sizeof(*entry.slot), previous, &ignored));
}

static const wchar_t *without_prefix(const wchar_t *path) {
    return wcsncmp(path, L"\\\\?\\", 4) == 0 ? path + 4 : path;
}

static int is_target(HANDLE handle) {
    DWORD saved = GetLastError();
    wchar_t actual[32768];
    DWORD length = GetFinalPathNameByHandleW(handle, actual, (DWORD)_countof(actual),
                                             FILE_NAME_NORMALIZED | VOLUME_NAME_DOS);
    int match = length > 0 && length < _countof(actual) &&
        CompareStringOrdinal(without_prefix(actual), -1, without_prefix(target_path), -1,
                             TRUE) == CSTR_EQUAL;
    SetLastError(saved);
    return match;
}

static BOOL WINAPI observed_flush(HANDLE handle) {
    if (is_target(handle)) {
        ++syncs;
        if (selected >= 1 && selected <= 3 && syncs == selected) {
            ++hits;
            win_error = ERROR_IO_DEVICE;
            SetLastError(ERROR_IO_DEVICE);
            return FALSE;
        }
    }
    return real_flush(handle);
}

static NTSTATUS NTAPI observed_write(HANDLE handle, HANDLE event, PIO_APC_ROUTINE apc,
                                     PVOID context, PIO_STATUS_BLOCK io, const void *bytes,
                                     ULONG length, const LARGE_INTEGER *offset, const ULONG *key) {
    if (is_target(handle)) {
        assert(offset != NULL && offset->QuadPart >= 4096);
        ++writes;
        const LONGLONG at = offset->QuadPart;
        if (at < 12288) ++roots;
        else if (at < 20480) ++witnesses;
        if ((selected == 4 && at < 12288) || (selected == 5 && at >= 20480)) {
            ++hits;
            win_error = selected == 4 ? ERROR_IO_DEVICE : ERROR_DISK_FULL;
            /* NTSTATUS equivalents of those Win32 errors. */
            io->Status = (NTSTATUS)(selected == 4 ? 0xc0000185u : 0xc000007fu);
            io->Information = 0;
            return io->Status;
        }
    }
    return real_write(handle, event, apc, context, io, bytes, length, offset, key);
}

static wchar_t *wide(const char *text) {
    int length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, -1, NULL, 0);
    assert(length > 0);
    wchar_t *out = (wchar_t *)malloc((size_t)length * sizeof(*out));
    assert(out != NULL);
    assert(MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, -1, out, length) == length);
    return out;
}

static void windows_arm(const char *path, int mode) {
    wchar_t *name = wide(path);
    DWORD size = GetFullPathNameW(name, (DWORD)_countof(target_path), target_path, NULL);
    free(name);
    assert(size > 0 && size < _countof(target_path));
    selected = mode;
#ifdef WDB_DISABLE_FAULTS
    selected = 0;
#endif
    hits = syncs = roots = witnesses = writes = win_error = 0;
    HMODULE module = GetModuleHandleW(L"weekend_db.dll");
    assert(module != NULL);
    flush_fn flush = observed_flush;
    write_fn write_at = observed_write;
    ULONG_PTR flush_address, write_address;
    _Static_assert(sizeof(flush) == sizeof(flush_address), "native function address");
    _Static_assert(sizeof(write_at) == sizeof(write_address), "native function address");
    memcpy(&flush_address, &flush, sizeof(flush_address));
    memcpy(&write_address, &write_at, sizeof(write_address));
    flush_patch = find_import(module, "FlushFileBuffers", flush_address);
    write_patch = find_import(module, "NtWriteFile", write_address);
    memcpy(&real_flush, &flush_patch.original, sizeof(real_flush));
    memcpy(&real_write, &write_patch.original, sizeof(real_write));
    patch(flush_patch, 1);
    patch(write_patch, 1);
}

static void windows_disarm(void) {
    patch(write_patch, 0);
    patch(flush_patch, 0);
}

static int windows_count(int key) {
    switch (key) {
    case 0: return hits;
    case 1: return syncs;
    case 2: return roots;
    case 3: return witnesses;
    case 4: return writes;
    case 5: return win_error;
    default: assert(0); return -1;
    }
}

static void windows_probe_unrelated(const char *path) {
    wchar_t *name = wide(path);
    HANDLE file = CreateFileW(name, GENERIC_WRITE, 0, NULL, CREATE_NEW,
                              FILE_ATTRIBUTE_NORMAL, NULL);
    free(name);
    assert(file != INVALID_HANDLE_VALUE);
    assert(observed_flush(file));
    assert(CloseHandle(file));
}
