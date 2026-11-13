/* Private single-threaded Linux fixture, never linked into the delivered library. */
#define _GNU_SOURCE
#include <assert.h>
#include <dlfcn.h>
#include <errno.h>
#include <stdint.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
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
int fsync(int fd) {
    static int (*next)(int);
    if(!next) { next=(int(*)(int))dlsym(RTLD_NEXT,"fsync");assert(next); }
    if(target(fd)) {
        ++syncs;
        if(!fired && mode>=1 && mode<=3 && syncs==mode) {fired=1;errno=EIO;return -1;}
    }
    return next(fd);
}
static int before_write(int fd, off64_t at) {
    if(!target(fd))return 0;
    ++writes;
    if(at>=4096 && at<12288) {
        ++roots;
        if(mode==4 && !fired) {fired=1;errno=EIO;return 1;}
    }
    if(at>=12288 && at<20480)++witnesses;
    return 0;
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
