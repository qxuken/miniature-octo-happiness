/* Private interposer for single-threaded compaction children. */
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#else
#define _GNU_SOURCE
#endif
#include <assert.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static struct stat directory, original, replacement;
static char scratch[50]; /* ".wdb-compact-" + 32 hex digits + ".tmp" + NUL */
static int active, selected, fault, step, exchanged;

static int same(const struct stat *a, const struct stat *b) {
    return a->st_dev == b->st_dev && a->st_ino == b->st_ino;
}

__attribute__((constructor)) static void arm(void) {
    const char *dir = getenv("WDB_NAMESPACE_DIRECTORY");
    if (!dir) return;
    assert(stat(dir, &directory) == 0 && S_ISDIR(directory.st_mode));
    int fd = open(dir, O_RDONLY | O_DIRECTORY);
    assert(fd >= 0);
    assert(fstatat(fd, "store.wdb", &original, AT_SYMLINK_NOFOLLOW) == 0);
    assert(S_ISREG(original.st_mode));
    assert(close(fd) == 0);
    const char *phase = getenv("WDB_NAMESPACE_STEP");
    const char *name = getenv("WDB_NAMESPACE_ERRNO");
    assert(phase && phase[0] >= '0' && phase[0] <= '7' && !phase[1] && name);
    selected = phase[0] - '0';
    if (strcmp(name, "ENOSPC") == 0) fault = ENOSPC;
    else if (strcmp(name, "EDQUOT") == 0) fault = EDQUOT;
    else {
        assert(strcmp(name, "EIO") == 0);
        fault = EIO;
    }
    active = 1;
}

static int fd_matches(int fd, const struct stat *wanted) {
    int saved = errno;
    struct stat fd_info;
    int match = fstat(fd, &fd_info) == 0 && same(&fd_info, wanted);
    errno = saved;
    return match;
}

static int inject(void) {
    fprintf(stderr, "namespace fault %d %d\n", selected, fault);
    errno = fault;
    return -1;
}

/* Reject wrong order even when the target operation returned success. */
static int observe(int phase) {
    assert(phase == step + 1);
    step = phase;
    fprintf(stderr, "namespace syscall %d\n", phase);
    return selected == phase;
}

static int sync_phase(int fd) {
    if (!active || !exchanged) return 0;
    if (fd_matches(fd, &directory)) return step == 1 ? 2 : 5;
    if (fd_matches(fd, &replacement)) return step == 2 ? 3 : 6;
    return 0;
}

typedef int (*exchange_fn)(int, const char *, int, const char *, unsigned int);
static int observed_exchange(exchange_fn next, int from_fd, const char *from,
                             int to_fd, const char *to, unsigned int flags) {
    if (!active || !fd_matches(from_fd, &directory) || strcmp(from, "store.wdb") != 0)
        return next(from_fd, from, to_fd, to, flags);
    assert(fd_matches(to_fd, &directory) && flags == 2);
    assert(strlen(to) + 1 == sizeof(scratch) && strncmp(to, ".wdb-compact-", 13) == 0);
    memcpy(scratch, to, sizeof(scratch));
    struct stat current;
    assert(fstatat(from_fd, from, &current, AT_SYMLINK_NOFOLLOW) == 0);
    assert(fstatat(to_fd, to, &replacement, AT_SYMLINK_NOFOLLOW) == 0);
    assert(same(&current, &original) && !same(&current, &replacement));
    if (observe(1)) return inject();
    int result = next(from_fd, from, to_fd, to, flags);
    if (result == 0) {
        exchanged = 1;
        if (selected == 7) return inject();
    }
    return result;
}

typedef int (*remove_fn)(int, const char *, int);
static int observed_remove(remove_fn next, int fd, const char *name, int flags) {
    if (active && exchanged && fd_matches(fd, &directory) && strcmp(scratch, name) == 0) {
        struct stat old;
        assert(flags == 0 && fstatat(fd, name, &old, AT_SYMLINK_NOFOLLOW) == 0);
        assert(same(&old, &original));
        if (observe(4)) return inject();
    }
    return next(fd, name, flags);
}

#if defined(__APPLE__)
static int observed_fcntl(int fd, int command, ...) {
    va_list args;
    switch (command) {
    case F_FULLFSYNC: {
        int phase = sync_phase(fd);
        if (phase && observe(phase)) return inject();
        return fcntl(fd, command);
    }
    case F_GETFD:
    case F_GETFL:
        return fcntl(fd, command);
    case F_GETPATH: {
        va_start(args, command);
        char *path = va_arg(args, char *);
        va_end(args);
        return fcntl(fd, command, path);
    }
    case F_SETFD:
    case F_SETFL:
    case F_DUPFD:
    case F_DUPFD_CLOEXEC: {
        va_start(args, command);
        int value = va_arg(args, int);
        va_end(args);
        return fcntl(fd, command, value);
    }
    default:
        fprintf(stderr, "unexpected namespace fcntl command %d\n", command);
        abort();
    }
}

static int observed_rename(int from_fd, const char *from,
                           int to_fd, const char *to, unsigned int flags) {
    return observed_exchange(renameatx_np, from_fd, from, to_fd, to, flags);
}

static int observed_unlinkat(int fd, const char *name, int flags) {
    return observed_remove(unlinkat, fd, name, flags);
}

#define INTERPOSE(replacement_fn, original_fn)                              \
    __attribute__((used, section("__DATA,__interpose"))) static const struct { \
        const void *replacement_address, *original_address;                  \
    } interpose_##original_fn = {                                             \
        (const void *)(uintptr_t)&replacement_fn,                            \
        (const void *)(uintptr_t)&original_fn                                  \
    }
INTERPOSE(observed_fcntl, fcntl);
INTERPOSE(observed_rename, renameatx_np);
INTERPOSE(observed_unlinkat, unlinkat);

#elif defined(__linux__)
int fsync(int fd) {
    static int (*next)(int);
    if (!next) {
        next = (int (*)(int))dlsym(RTLD_NEXT, "fsync");
        assert(next);
    }
    int phase = sync_phase(fd);
    if (phase && observe(phase)) return inject();
    return next(fd);
}

int renameat2(int from_fd, const char *from,
              int to_fd, const char *to, unsigned int flags) {
    static exchange_fn next;
    if (!next) {
        next = (exchange_fn)dlsym(RTLD_NEXT, "renameat2");
        assert(next);
    }
    return observed_exchange(next, from_fd, from, to_fd, to, flags);
}

int unlinkat(int fd, const char *name, int flags) {
    static remove_fn next;
    if (!next) {
        next = (remove_fn)dlsym(RTLD_NEXT, "unlinkat");
        assert(next);
    }
    return observed_remove(next, fd, name, flags);
}
#else
#error unsupported namespace fixture host
#endif
