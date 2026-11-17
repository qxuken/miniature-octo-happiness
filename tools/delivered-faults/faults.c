/* Private single-threaded fixture, loaded only by the exercising consumer. */
#if defined(__APPLE__)
#define _DARWIN_C_SOURCE
#include <fcntl.h>
#include <stdarg.h>
#include <stdio.h>
#else
#define _GNU_SOURCE
#endif
#include <assert.h>
#include <dlfcn.h>
#include <errno.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
#if defined(__linux__)
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/statfs.h>
#include <sys/statvfs.h>
#endif
static dev_t device;
static ino_t inode;
static int tracking, mode, fired, syncs, roots, witnesses, writes, native_error;
void qualification_arm(const char *path, int selected) {
    struct stat st; assert(stat(path,&st)==0);
    device=st.st_dev;inode=st.st_ino;mode=selected;tracking=1;
#ifdef QUALIFICATION_BYPASS_INJECTION
    mode=0;
#endif
    fired=syncs=roots=witnesses=writes=native_error=0;
}
void qualification_disarm(void) { tracking=0; }
int qualification_count(int kind) {
    switch(kind) {
    case 0:return fired;case 1:return syncs;case 2:return roots;
    case 3:return witnesses;case 4:return writes;case 5:return native_error;
    default:assert(0);return -1;
    }
}
static int target(int fd) {
    if(!tracking)return 0;
    int saved=errno;struct stat st;
    int yes=fstat(fd,&st)==0 && st.st_dev==device && st.st_ino==inode;
    errno=saved;return yes;
}
static int before_sync(int fd) {
    if (!target(fd)) return 0;
    ++syncs;
    if (!fired && mode >= 1 && mode <= 3 && syncs == mode) {
        fired = 1;
        errno = EIO;
        return 1;
    }
    return 0;
}
static int before_write(int fd, int64_t at) {
    if(!target(fd))return 0;
    ++writes;
    if(at>=4096 && at<12288) {
        ++roots;
        if(mode==4 && !fired) {fired=1;errno=EIO;return 1;}
    }
    if(at>=12288 && at<20480)++witnesses;
    return 0;
}
#if defined(__APPLE__)
static int observed_fcntl(int fd, int command, ...) {
    va_list args;
    switch (command) {
    case F_FULLFSYNC:
        if (before_sync(fd)) return -1;
        return fcntl(fd, command);
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
        int flags = va_arg(args, int);
        va_end(args);
        return fcntl(fd, command, flags);
    }
    default:
        fprintf(stderr, "unexpected fcntl command %d in delivered fixture\n", command);
        assert(0);
        return -1;
    }
}

static ssize_t observed_pwrite(int fd, const void *bytes, size_t length, off_t at) {
    if (before_write(fd, at)) return -1;
    ssize_t result = pwrite(fd, bytes, length, at);
    if (result < 0 && target(fd)) native_error = errno;
    return result;
}

/* dyld leaves calls to the original symbol in the interposer itself intact. */
#define INTERPOSE(replacement, original)                                      \
    __attribute__((used, section("__DATA,__interpose"))) static const struct { \
        const void *replacement_address;                                     \
        const void *original_address;                                        \
    } interpose_##original = {                                                \
        (const void *)(uintptr_t)&replacement, (const void *)(uintptr_t)&original \
    }
INTERPOSE(observed_fcntl, fcntl);
INTERPOSE(observed_pwrite, pwrite);
#elif defined(__linux__)
void qualification_fill_tmpfs(const char *directory) {
    const char *enabled = getenv("QUALIFICATION_PRIVATE_TMPFS");
    assert(enabled && !strcmp(enabled, "1"));
    struct statfs fs;
    struct statvfs space;
    assert(statfs(directory, &fs) == 0 && (unsigned long)fs.f_type == 0x01021994ul);
    assert(statvfs(directory, &space) == 0 && space.f_frsize > 0);
    assert(space.f_blocks <= (8u * 1024 * 1024) / space.f_frsize);

    char path[1024];
    int length = snprintf(path, sizeof path, "%s/ballast", directory);
    assert(length > 0 && (size_t)length < sizeof path);
    int fd = open(path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
    assert(fd >= 0);
    uint8_t block[65536];
    memset(block, 0x5a, sizeof block);
    int full = 0;
    for (int i = 0; i < 256 && !full; ++i) {
        size_t written = 0;
        while (written < sizeof block) {
            ssize_t count = write(fd, block + written, sizeof block - written);
            if (count < 0) {
                assert(errno == ENOSPC);
                full = 1;
                break;
            }
            assert(count > 0);
            written += (size_t)count;
        }
    }
    assert(full);
    assert(close(fd) == 0);
}

int fsync(int fd) {
    static int (*next)(int);
    if(!next) { next=(int(*)(int))dlsym(RTLD_NEXT,"fsync");assert(next); }
    if (before_sync(fd)) return -1;
    return next(fd);
}
ssize_t pwrite(int fd,const void *bytes,size_t length,off_t at) {
    static ssize_t (*next)(int,const void*,size_t,off_t);
    if(!next) {next=(ssize_t(*)(int,const void*,size_t,off_t))dlsym(RTLD_NEXT,"pwrite");assert(next);}
    if(before_write(fd,at))return -1;
    ssize_t result=next(fd,bytes,length,at);
    if(result<0 && target(fd))native_error=errno;
    return result;
}
ssize_t pwrite64(int fd,const void *bytes,size_t length,off64_t at) {
    static ssize_t (*next)(int,const void*,size_t,off64_t);
    if(!next) {next=(ssize_t(*)(int,const void*,size_t,off64_t))dlsym(RTLD_NEXT,"pwrite64");assert(next);}
    if(before_write(fd,at))return -1;
    ssize_t result=next(fd,bytes,length,at);
    if(result<0 && target(fd))native_error=errno;
    return result;
}
#else
#error unsupported delivered fault fixture host
#endif
