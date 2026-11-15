/* Pause the production file adapter just after creating one exclusive target.
 * I/O errors come from the independently armed device-mapper table. */
#define _GNU_SOURCE
#include <assert.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

static struct stat directory;
static const char *label, *gate, *target;
static int armed;

static int scratch_name(const char *name) {
    static const char prefix[] = ".wdb-compact-";
    size_t prefix_len = sizeof(prefix) - 1;
    return strlen(name) == prefix_len + 32 + 4
        && strncmp(name, prefix, prefix_len) == 0
        && strspn(name + prefix_len, "0123456789abcdef") == 32
        && strcmp(name + prefix_len + 32, ".tmp") == 0;
}

__attribute__((constructor)) static void setup(void) {
    const char *dir = getenv("WDB_WRITEBACK_DIRECTORY");
    gate = getenv("WDB_WRITEBACK_GATE");
    label = getenv("WDB_WRITEBACK_LABEL");
    target = getenv("WDB_WRITEBACK_TARGET");
    assert(dir && gate && label && target && getuid() != 0);
    assert(stat(dir, &directory) == 0 && S_ISDIR(directory.st_mode));
}

static void await_arm(int fd, const char *name) {
    struct stat info;
    assert(!armed);
    armed = 1;
    assert(fstat(fd, &info) == 0 && S_ISREG(info.st_mode));
    assert(info.st_dev == directory.st_dev && info.st_uid == getuid());
    assert(info.st_size == 0 && info.st_nlink == 1);
    assert(fallocate(fd, FALLOC_FL_KEEP_SIZE, 0, 4096) == 0);
    assert(fstat(fd, &info) == 0 && info.st_size == 0);
    printf("ready %s %jd %ju %ju %s\n", label, (intmax_t)getpid(),
           (uintmax_t)info.st_dev, (uintmax_t)info.st_ino, name);
    assert(fflush(stdout) == 0);

    struct timespec started, now, pause = {0, 10000000};
    assert(clock_gettime(CLOCK_MONOTONIC, &started) == 0);
    for (;;) {
        if (lstat(gate, &info) == 0) {
            assert(S_ISDIR(info.st_mode));
            return;
        }
        assert(errno == ENOENT);
        assert(clock_gettime(CLOCK_MONOTONIC, &now) == 0);
        assert(now.tv_sec - started.tv_sec < 25);
        nanosleep(&pause, NULL);
    }
}

int openat(int dirfd, const char *name, int flags, ...) {
    static int (*next)(int, const char *, int, ...);
    if (!next) {
        next = (int (*)(int, const char *, int, ...))dlsym(RTLD_NEXT, "openat");
        assert(next);
    }
    int fd;
    if (flags & O_CREAT || (flags & O_TMPFILE) == O_TMPFILE) {
        va_list args;
        va_start(args, flags);
        int mode = va_arg(args, int);
        va_end(args);
        fd = next(dirfd, name, flags, mode);
    } else {
        fd = next(dirfd, name, flags);
    }
    if (target && fd >= 0 && (flags & (O_CREAT | O_EXCL)) == (O_CREAT | O_EXCL)
        && (strcmp(target, "scratch") == 0 ? scratch_name(name) : strcmp(target, name) == 0)) {
        struct stat parent;
        assert(fstat(dirfd, &parent) == 0 && parent.st_dev == directory.st_dev
               && parent.st_ino == directory.st_ino);
        await_arm(fd, name);
    }
    return fd;
}
