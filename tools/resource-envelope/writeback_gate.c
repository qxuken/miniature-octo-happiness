/* Pause the production file adapter at the selected writeback boundary.
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

static struct stat directory, created, original;
static const char *label, *gate, *target, *metadata_magic;
static off64_t offset;
static char name_at_creation[64];
static int seen_target, armed;
static enum { DATA, COPY_ROOT, FILE_METADATA, EXCHANGE_DIRECTORY, CLEANUP_DIRECTORY } phase;

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
    const char *barrier = getenv("WDB_WRITEBACK_BARRIER");
    assert(dir && gate && label && target && barrier && getuid() != 0);
    assert(stat(dir, &directory) == 0 && S_ISDIR(directory.st_mode));
    if (strcmp(barrier, "slot") == 0) {
        phase = COPY_ROOT;
        offset = 4096;
        metadata_magic = "WDBSLOT1";
    } else if (strcmp(barrier, "witness") == 0) {
        phase = COPY_ROOT;
        offset = 12288;
        metadata_magic = "WDBACK01";
    } else if (strcmp(barrier, "data") == 0) {
        phase = DATA;
    } else {
        assert(strcmp(target, "scratch") == 0);
        if (strcmp(barrier, "file-metadata") == 0) {
            phase = FILE_METADATA;
        } else if (strcmp(barrier, "exchange-directory") == 0) {
            phase = EXCHANGE_DIRECTORY;
        } else {
            assert(strcmp(barrier, "cleanup-directory") == 0);
            phase = CLEANUP_DIRECTORY;
        }
    }
}

static int same_inode(const struct stat *a, const struct stat *b) {
    return a->st_dev == b->st_dev && a->st_ino == b->st_ino;
}

static void await_arm(const struct stat *entry) {
    struct stat info;
    assert(!armed);
    armed = 1;
    printf("ready %s %jd %ju %ju %s\n", label, (intmax_t)getpid(),
           (uintmax_t)entry->st_dev, (uintmax_t)entry->st_ino, name_at_creation);
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
        assert(fstat(dirfd, &parent) == 0 && same_inode(&parent, &directory));
        assert(!seen_target);
        assert(fstat(fd, &created) == 0 && S_ISREG(created.st_mode));
        assert(created.st_dev == directory.st_dev && created.st_uid == getuid());
        assert(created.st_size == 0 && created.st_nlink == 1);
        assert(strlen(name) < sizeof(name_at_creation));
        memcpy(name_at_creation, name, strlen(name) + 1);
        seen_target = 1;
        if (phase == DATA) {
            assert(fallocate(fd, FALLOC_FL_KEEP_SIZE, 0, 4096) == 0);
            struct stat info;
            assert(fstat(fd, &info) == 0 && info.st_size == 0);
            await_arm(&created);
        }
    }
    return fd;
}

ssize_t pwrite64(int fd, const void *buf, size_t size, off64_t where) {
    static ssize_t (*next)(int, const void *, size_t, off64_t);
    if (!next) {
        next = (ssize_t (*)(int, const void *, size_t, off64_t))dlsym(RTLD_NEXT, "pwrite64");
        assert(next);
    }
    /* Initial file creation writes zeroes at the same offset. Pause only
     * before writing the populated root slot or witness to the new inode. */
    if (seen_target && phase == COPY_ROOT && where == offset && size == 4096
        && memcmp(buf, metadata_magic, 8) == 0) {
        struct stat info;
        assert(fstat(fd, &info) == 0);
        if (same_inode(&info, &created)) {
            assert(info.st_size == 63074304);
            await_arm(&created);
        }
    }
    return next(fd, buf, size, where);
}

int fchmod(int fd, mode_t mode) {
    static int (*next)(int, mode_t);
    if (!next) {
        next = (int (*)(int, mode_t))dlsym(RTLD_NEXT, "fchmod");
        assert(next);
    }
    if (seen_target && phase == FILE_METADATA && mode == 0640) {
        struct stat info;
        assert(fstat(fd, &info) == 0);
        if (same_inode(&info, &created)) {
            assert(info.st_size == 63074304 && (info.st_mode & 07777) == 0600);
            await_arm(&created);
            int result = next(fd, mode);
            assert(result == 0);
            assert(fstat(fd, &info) == 0 && (info.st_mode & 07777) == mode);
            return result;
        }
    }
    return next(fd, mode);
}

static struct stat owned_entry(int dirfd, const char *name) {
    struct stat parent, info;
    assert(fstat(dirfd, &parent) == 0 && same_inode(&parent, &directory));
    assert(fstatat(dirfd, name, &info, AT_SYMLINK_NOFOLLOW) == 0);
    assert(S_ISREG(info.st_mode) && info.st_dev == directory.st_dev
           && info.st_uid == getuid() && info.st_nlink == 1);
    return info;
}

int renameat2(int from, const char *old, int to, const char *new, unsigned int flags) {
    static int (*next)(int, const char *, int, const char *, unsigned int);
    if (!next) {
        next = (int (*)(int, const char *, int, const char *, unsigned int))dlsym(RTLD_NEXT, "renameat2");
        assert(next);
    }
    if (seen_target && (phase == EXCHANGE_DIRECTORY || phase == CLEANUP_DIRECTORY)
        && strcmp(old, "source.wdb") == 0 && strcmp(new, name_at_creation) == 0) {
        assert(flags == RENAME_EXCHANGE);
        original = owned_entry(from, old);
        struct stat scratch = owned_entry(to, new);
        assert(same_inode(&scratch, &created));
        if (phase == EXCHANGE_DIRECTORY) {
            await_arm(&scratch);
        }
        int result = next(from, old, to, new, flags);
        assert(result == 0);
        struct stat live_source = owned_entry(from, old);
        struct stat live_scratch = owned_entry(to, new);
        assert(same_inode(&live_source, &created) && same_inode(&live_scratch, &original));
        return result;
    }
    return next(from, old, to, new, flags);
}

int unlinkat(int dirfd, const char *name, int flags) {
    static int (*next)(int, const char *, int);
    if (!next) {
        next = (int (*)(int, const char *, int))dlsym(RTLD_NEXT, "unlinkat");
        assert(next);
    }
    if (seen_target && phase == CLEANUP_DIRECTORY && strcmp(name, name_at_creation) == 0) {
        assert(flags == 0);
        struct stat scratch = owned_entry(dirfd, name);
        struct stat source = owned_entry(dirfd, "source.wdb");
        assert(same_inode(&scratch, &original) && same_inode(&source, &created));
        await_arm(&scratch);
        int result = next(dirfd, name, flags);
        assert(result == 0);
        assert(fstatat(dirfd, name, &scratch, AT_SYMLINK_NOFOLLOW) == -1 && errno == ENOENT);
        return result;
    }
    return next(dirfd, name, flags);
}
