/* Windows transport/loader shell around the unchanged delivered-API consumer. */
#ifndef _WIN32
#error "This consumer requires native Windows"
#endif
#define WIN32_LEAN_AND_MEAN
#define _CRT_SECURE_NO_WARNINGS
#include <windows.h>
#include <bcrypt.h>
#include <assert.h>
#include <fcntl.h>
#include <io.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

static char *utf8(const wchar_t *text) {
    int n = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, -1, NULL, 0, NULL, NULL);
    assert(n > 0);
    char *out = (char *)malloc((size_t)n);
    assert(out != NULL);
    assert(WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, text, -1, out, n, NULL, NULL) == n);
    return out;
}

static FILE *utf8_fopen(const char *path, const char *mode) {
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, NULL, 0);
    assert(n > 0);
    wchar_t *wide = (wchar_t *)malloc((size_t)n * sizeof(wchar_t));
    assert(wide != NULL);
    assert(MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, n) == n);
    /* The original consumer only opens its binary trace for reading. */
    assert(strcmp(mode, "rb") == 0);
    FILE *file = _wfopen(wide, L"rb");
    free(wide);
    return file;
}

#define fopen utf8_fopen
#define main trace_main
#include "consumer.c"
#undef main
#undef fopen

static void file_sha256(HANDLE file, char out[65]) {
    BCRYPT_ALG_HANDLE algorithm = NULL;
    BCRYPT_HASH_HANDLE hash = NULL;
    ULONG object_size = 0, returned = 0;
    BYTE digest[32], buffer[65536];
    LARGE_INTEGER zero, size;
    zero.QuadPart = 0;
    assert(GetFileSizeEx(file, &size));
    assert(size.QuadPart > 0 && size.QuadPart <= 256 * 1024 * 1024);
    assert(SetFilePointerEx(file, zero, NULL, FILE_BEGIN));
    assert(BCryptOpenAlgorithmProvider(&algorithm, BCRYPT_SHA256_ALGORITHM, NULL, 0) >= 0);
    assert(BCryptGetProperty(algorithm, BCRYPT_OBJECT_LENGTH, (PUCHAR)&object_size,
                           (ULONG)sizeof(object_size), &returned, 0) >= 0);
    assert(returned == sizeof(object_size) && object_size > 0 && object_size <= 1024 * 1024);
    PUCHAR object = (PUCHAR)malloc(object_size);
    assert(object != NULL);
    assert(BCryptCreateHash(algorithm, &hash, object, object_size, NULL, 0, 0) >= 0);
    LONGLONG total = 0;
    for (;;) {
        DWORD count = 0;
        assert(ReadFile(file, buffer, (DWORD)sizeof(buffer), &count, NULL));
        if (count == 0) break;
        total += count;
        assert(total <= size.QuadPart);
        assert(BCryptHashData(hash, buffer, count, 0) >= 0);
    }
    assert(total == size.QuadPart);
    assert(BCryptFinishHash(hash, digest, (ULONG)sizeof(digest), 0) >= 0);
    assert(BCryptDestroyHash(hash) >= 0);
    assert(BCryptCloseAlgorithmProvider(algorithm, 0) >= 0);
    free(object);
    for (size_t i = 0; i < sizeof(digest); i++) {
        static const char digits[] = "0123456789abcdef";
        out[i * 2] = digits[digest[i] >> 4];
        out[i * 2 + 1] = digits[digest[i] & 15];
    }
    out[64] = '\0';
}

static void json_string(FILE *file, const char *text, size_t length) {
    assert(fputc('"', file) != EOF);
    for (size_t i = 0; i < length; i++) {
        unsigned char byte = (unsigned char)text[i];
        if (byte == '"' || byte == '\\') {
            assert(fputc('\\', file) != EOF);
            assert(fputc(byte, file) != EOF);
        } else if (byte < 32) {
            assert(fprintf(file, "\\u%04x", (unsigned int)byte) > 0);
        } else {
            assert(fputc(byte, file) != EOF);
        }
    }
    assert(fputc('"', file) != EOF);
}

static void attest(const wchar_t *proof, const char *phase, const wchar_t *loaded,
                   const char *digest, const char *expected_source) {
    uint8_t source[256];
    size_t length = 0;
    OK(wdb_source_id(source, sizeof(source), &length));
    assert(length == strlen(expected_source) && memcmp(source, expected_source, length) == 0);
    assert(wdb_abi_version() == WDB_ABI_VERSION && wdb_format_version() == WDB_FORMAT_VERSION);
    HANDLE handle = CreateFileW(proof, GENERIC_WRITE, 0, NULL, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, NULL);
    assert(handle != INVALID_HANDLE_VALUE);
    int fd = _open_osfhandle((intptr_t)handle, _O_WRONLY | _O_BINARY);
    assert(fd >= 0);
    FILE *file = _fdopen(fd, "wb");
    assert(file != NULL);
    char *path = utf8(loaded);
    assert(fprintf(file, "{\"schema\":1,\"phase\":\"%s\",\"pid\":%lu,\"loaded_dll\":", phase,
                   (unsigned long)GetCurrentProcessId()) > 0);
    json_string(file, path, strlen(path));
    assert(fprintf(file, ",\"sha256\":\"%s\",\"source_id\":", digest) > 0);
    json_string(file, (const char *)source, length);
    assert(fprintf(file, ",\"abi\":%u,\"format\":%u,\"features\":%u}\n",
                   wdb_abi_version(), wdb_format_version(), wdb_features()) > 0);
    assert(fclose(file) == 0); /* Also closes the native handle. */
    free(path);
}

int wmain(int argc, wchar_t **wide_argv) {
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
    _set_error_mode(_OUT_TO_STDERR);
    _set_abort_behavior(0, _CALL_REPORTFAULT);
    assert(argc == 8);
    assert(_setmode(_fileno(stdout), _O_BINARY) != -1); /* Canonical LF transcript. */
    wchar_t loaded[32768], expected[32768];
    HMODULE module = GetModuleHandleW(L"weekend_db.dll");
    assert(module != NULL && GetProcAddress(module, "wdb_source_id") != NULL);
    DWORD length = GetModuleFileNameW(module, loaded, (DWORD)(sizeof(loaded) / sizeof(loaded[0])));
    assert(length > 0 && length < sizeof(loaded) / sizeof(loaded[0]));
    DWORD expected_length = GetFullPathNameW(wide_argv[4], (DWORD)(sizeof(expected) / sizeof(expected[0])), expected, NULL);
    assert(expected_length > 0 && expected_length < sizeof(expected) / sizeof(expected[0]));
    assert(CompareStringOrdinal(loaded, (int)length, expected, (int)expected_length, TRUE) == CSTR_EQUAL);
    /* Deny writes/deletion of the actual loaded file throughout the trace. */
    HANDLE file = CreateFileW(loaded, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    assert(file != INVALID_HANDLE_VALUE);
    char before[65], after[65];
    file_sha256(file, before);
    char *argv[8];
    for (int i = 0; i < argc; i++) argv[i] = utf8(wide_argv[i]);
    assert(strlen(argv[5]) == 64 && strcmp(before, argv[5]) == 0);
    attest(wide_argv[6], "before", loaded, before, argv[3]);
    int result = trace_main(4, argv);
    assert(result == 0);
    file_sha256(file, after);
    assert(strcmp(before, after) == 0);
    attest(wide_argv[7], "after", loaded, after, argv[3]);
    assert(CloseHandle(file));
    for (int i = 0; i < argc; i++) free(argv[i]);
    return result;
}
