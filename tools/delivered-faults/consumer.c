#define _GNU_SOURCE
#include "weekend_db.h"
#include <assert.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/statfs.h>
#include <sys/statvfs.h>
#include <unistd.h>
#ifdef NDEBUG
#error assertions required
#endif
#define OK(call) assert((call)==WDB_OK)
static void (*arm)(const char*,int);
static void (*disarm)(void);
static int (*count)(int);
static wdb_value *bytes(wdb_database *db,unsigned char byte,size_t length) {
    uint8_t buffer[65536];assert(length<=sizeof buffer);memset(buffer,byte,length);
    wdb_value *value=NULL;OK(wdb_value_make(db,WDB_BYTES,0,buffer,length,&value));return value;
}
static void put(wdb_tx *tx,const char *key,wdb_value *value) {
    OK(wdb_tx_put(tx,1,(const uint8_t*)key,strlen(key),value));
}
static void blob(wdb_snapshot *view,const char *key,int byte,size_t length) {
    wdb_value *value=NULL;OK(wdb_snapshot_get(view,1,(const uint8_t*)key,strlen(key),&value));
    uint8_t buffer[65536];size_t required=0;
    OK(wdb_value_bytes(value,buffer,sizeof buffer,&required));assert(required==length);
    for(size_t i=0;i<length;++i)assert(buffer[i]==byte);
    OK(wdb_value_release(value));
}
static void verify_view(wdb_snapshot *view,uint64_t revision,int candidate) {
    uint64_t actual;OK(wdb_snapshot_revision(view,&actual));assert(actual==revision);
    wdb_scan *rows=NULL;size_t n;OK(wdb_snapshot_scan(view,1,&rows));
    OK(wdb_scan_len(rows,&n));assert(n==(candidate?3u:1u));
    const char *keys_new[]={"payload","request","row"};
    for(size_t i=0;i<n;++i) {
        uint8_t key[32];size_t size=99;OK(wdb_scan_key(rows,i,key,sizeof key,&size));
        const char *expected=candidate?keys_new[i]:"row";
        assert(size==strlen(expected) && !memcmp(key,expected,size));
    }
    OK(wdb_scan_release(rows));blob(view,"row",candidate?2:1,32768);
    wdb_value *marker=NULL;
    int status=wdb_snapshot_get(view,1,(const uint8_t*)"request",7,&marker);
    if(candidate) {
        uint64_t request;assert(status==WDB_OK && marker);
        OK(wdb_value_scalar(marker,&request));assert(request==42);
        OK(wdb_value_release(marker));blob(view,"payload",0xa5,65536);
    } else assert(status==WDB_NOT_FOUND && marker==NULL);
}
static void verify(wdb_database *db,uint64_t revision,int candidate) {
    wdb_snapshot *current=NULL,*old=NULL;OK(wdb_snapshot_acquire(db,&current));
    verify_view(current,revision,candidate);OK(wdb_snapshot_release(current));
    uint64_t ids[2];size_t n=99;OK(wdb_retained_ids(db,ids,2,&n));assert(n==1 && ids[0]==1);
    OK(wdb_retained_open(db,1,&old));verify_view(old,2,0);OK(wdb_snapshot_release(old));
}
static void candidate(wdb_database *db,wdb_tx **tx) {
    OK(wdb_tx_begin(db,WDB_RECORDS,tx));
    wdb_value *row=bytes(db,2,32768),*payload=bytes(db,0xa5,65536),*request=NULL;
    OK(wdb_value_make(db,WDB_UNSIGNED,42,NULL,0,&request));
    put(*tx,"row",row);put(*tx,"payload",payload);put(*tx,"request",request);
    OK(wdb_value_release(row));OK(wdb_value_release(payload));OK(wdb_value_release(request));
}
static void fill(const char *directory,char *ballast,size_t capacity) {
    assert(getenv("QUALIFICATION_PRIVATE_TMPFS") && !strcmp(getenv("QUALIFICATION_PRIVATE_TMPFS"),"1"));
    struct statfs fs;struct statvfs space;
    assert(statfs(directory,&fs)==0 && (unsigned long)fs.f_type==0x01021994ul);
    assert(statvfs(directory,&space)==0 && space.f_blocks*space.f_frsize<=8u*1024*1024);
    assert(snprintf(ballast,capacity,"%s/ballast",directory)>0);
    int fd=open(ballast,O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC,0600);assert(fd>=0);
    uint8_t block[65536];memset(block,0x5a,sizeof block);
    int full=0;
    for(int i=0;i<256 && !full;++i) {
        size_t written=0;
        while(written<sizeof block) {
            ssize_t n=write(fd,block+written,sizeof block-written);
            if(n<0) {assert(errno==ENOSPC);full=1;break;}
            assert(n>0);written+=(size_t)n;
        }
    }
    assert(full);assert(close(fd)==0);
}
int main(int argc,char **argv) {
    assert(argc==4 || argc==5);int recovering=argc==5;
    if(recovering)assert(!strcmp(argv[1],"recover"));
    int mode=atoi(recovering?argv[4]:argv[1]);assert(mode>=0 && mode<=5);
    if(!recovering) {
        arm=(void(*)(const char*,int))dlsym(RTLD_DEFAULT,"qualification_arm");
        disarm=(void(*)(void))dlsym(RTLD_DEFAULT,"qualification_disarm");
        count=(int(*)(int))dlsym(RTLD_DEFAULT,"qualification_count");assert(arm && disarm && count);
    }
    uint8_t source[64];size_t need=99;OK(wdb_source_id(source,sizeof source,&need));
    assert(need==strlen(argv[3]) && !memcmp(source,argv[3],need));
    assert(wdb_abi_version()==1 && wdb_format_version()==2 && wdb_features()==3);
    char path[1024],ballast[1024];assert(strlen(argv[2])<900);
    assert(snprintf(path,sizeof path,"%s/database.wdb",argv[2])>0);
    if(recovering) {
        /* A fresh, uninstrumented process reconciles the marker before retry. */
        wdb_database *reopened=NULL;wdb_tx *retry=NULL;uint64_t revision=99;
        OK(wdb_file_open((const uint8_t*)path,strlen(path),&reopened));
        int published=mode==0 || mode==2 || mode==3;
        verify(reopened,published?4:3,published);
        if(!published) {
            candidate(reopened,&retry);OK(wdb_tx_commit(retry,&revision));
            assert(revision==4);OK(wdb_tx_release(retry));
        }
        verify(reopened,4,1);OK(wdb_close(reopened));
        OK(wdb_file_open((const uint8_t*)path,strlen(path),&reopened));
        verify(reopened,4,1);OK(wdb_close(reopened));
        printf("{\"mode\":%d,\"reopened\":%d,\"final\":4,\"retained_revision\":2,\"verified\":true}\n",mode,published?4:3);
        return 0;
    }
    wdb_database *db=NULL;OK(wdb_file_create((const uint8_t*)path,strlen(path),&db));
    uint64_t collection=0,revision=99,id=0;OK(wdb_collection_create(db,&collection));assert(collection==1);
    wdb_tx *tx=NULL;OK(wdb_tx_begin(db,WDB_RECORDS,&tx));wdb_value *one=bytes(db,1,32768);
    put(tx,"row",one);OK(wdb_value_release(one));OK(wdb_tx_commit(tx,&revision));assert(revision==2);OK(wdb_tx_release(tx));
    wdb_snapshot *old=NULL;OK(wdb_snapshot_acquire(db,&old));OK(wdb_snapshot_retain(db,old,&id));assert(id==1);
    OK(wdb_snapshot_release(old));verify(db,3,0);
    if(mode==5)fill(argv[2],ballast,sizeof ballast);
    candidate(db,&tx);arm(path,mode==5?0:mode);
    /* A different inode must not consume this database's armed sync failure. */
    char other[1024];assert(snprintf(other,sizeof other,"%s/unrelated",argv[2])>0);
    if(mode!=5) {int fd=open(other,O_WRONLY|O_CREAT|O_EXCL,0600);assert(fd>=0);assert(fsync(fd)==0);assert(close(fd)==0);}
    assert(count(1)==0 && count(0)==0);
    revision=UINT64_C(0xdeadbeefcafebabe);int status=wdb_tx_commit(tx,&revision);
    int expected=mode==0?WDB_OK:(mode==1 || mode==5?WDB_IO:WDB_OUTCOME_UNKNOWN);
    assert(status==expected && revision==(mode==0?4u:0u));
    int hits=count(0),syncs=count(1),roots=count(2),witnesses=count(3),native_error=count(5);
    assert(hits==((mode>=1 && mode<=4)?1:0));assert(count(4)>0);
    if(mode==0)assert(syncs==3 && roots==1 && witnesses==1);
    if(mode>=1 && mode<=3)assert(syncs==mode && roots==(mode>=2) && witnesses==(mode>=3));
    if(mode==4)assert(syncs==1 && roots==1 && witnesses==0);
    if(mode==5)assert(syncs==0 && roots==0 && witnesses==0 && native_error==ENOSPC);
    verify(db,mode==0?4:3,mode==0);
    revision=99;assert(wdb_tx_commit(tx,&revision)==WDB_CLOSED && revision==0);OK(wdb_tx_release(tx));
    if(mode!=0) {
        OK(wdb_tx_begin(db,WDB_RECORDS,&tx));revision=99;
        assert(wdb_tx_commit(tx,&revision)==WDB_RECOVERY_REQUIRED && revision==0);
        revision=99;assert(wdb_tx_commit(tx,&revision)==WDB_CLOSED && revision==0);OK(wdb_tx_release(tx));
    }
    disarm();OK(wdb_close(db));if(mode==5)assert(unlink(ballast)==0);
    printf("{\"mode\":%d,\"status\":%d,\"visible\":%d,\"hits\":%d,\"syncs\":%d,\"root_writes\":%d,\"witness_writes\":%d,\"native_errno\":%d,\"verified\":true}\n",mode,status,mode==0?4:3,hits,syncs,roots,witnesses,native_error);
}
