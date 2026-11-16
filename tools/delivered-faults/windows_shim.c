/* Private dynamic entry points for the Odin fault consumer. */
#define WIN32_LEAN_AND_MEAN
#define _CRT_SECURE_NO_WARNINGS
#include <windows.h>
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include "windows_injector.h"

__declspec(dllexport) void qualification_arm(const char *path, int mode) {
    _set_error_mode(_OUT_TO_STDERR);
    _set_abort_behavior(0, _CALL_REPORTFAULT);
    windows_arm(path, mode);
}

__declspec(dllexport) void qualification_disarm(void) {
    windows_disarm();
}

__declspec(dllexport) int qualification_count(int key) {
    return windows_count(key);
}

__declspec(dllexport) void qualification_probe_unrelated(const char *path) {
    windows_probe_unrelated(path);
}

__declspec(dllexport) void qualification_fill_ntfs(const char *directory) {
    windows_fill(directory);
}
