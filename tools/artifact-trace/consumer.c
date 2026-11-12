#include "weekend_db.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef NDEBUG
#error "artifact trace requires runtime assertions"
#endif

#define OK(call) assert((call)==WDB_OK)
static void hex(const uint8_t *p,size_t n) { for(size_t i=0;i<n;i++) printf("%02x",p[i]); }
static void unchanged(const uint8_t *p,size_t n) { for(size_t i=0;i<n;i++) assert(p[i]==0xa5); }
static void value(wdb_value *v);
static void scan(wdb_scan *s,int map) {
    size_t count; OK(wdb_scan_len(s,&count)); assert(count<=16);
    printf(map?"m{":"[");
    for(size_t i=0;i<count;i++) {
        uint8_t key[32];size_t need=0;
        assert(wdb_scan_key(s,i,NULL,0,&need)==WDB_BUFFER && need>0 && need<=sizeof key);
        const size_t initial=need;
        memset(key,0xa5,sizeof key);
        need=99;
        assert(wdb_scan_key(s,i,key,initial-1,&need)==WDB_BUFFER && need==initial);
        unchanged(key,sizeof key);
        need=99;OK(wdb_scan_key(s,i,key,sizeof key,&need));assert(need==initial);
        if(i) { printf(map?",":";"); }
        hex(key,need);printf(map?":":"=");
        wdb_value *v=NULL;OK(wdb_scan_value(s,i,&v));value(v);OK(wdb_value_release(v));
    }
    printf(map?"}":"]");
}
static void value(wdb_value *v) {
    int32_t kind;OK(wdb_value_kind(v,&kind));
    if(kind==WDB_UNSIGNED) {uint64_t n;OK(wdb_value_scalar(v,&n));printf("u%llu",(unsigned long long)n);}
    else if(kind==WDB_BYTES) {
        uint8_t bytes[32];size_t n;
        assert(wdb_value_bytes(v,NULL,0,&n)==WDB_BUFFER && n>0 && n<=sizeof bytes);
        const size_t initial=n;
        memset(bytes,0xa5,sizeof bytes);
        n=99;assert(wdb_value_bytes(v,bytes,initial-1,&n)==WDB_BUFFER && n==initial);
        unchanged(bytes,sizeof bytes);
        n=99;OK(wdb_value_bytes(v,bytes,sizeof bytes,&n));assert(n==initial);printf("b");hex(bytes,n);
    } else if(kind==WDB_MAP) {wdb_scan *s;OK(wdb_map_scan(v,&s));scan(s,1);OK(wdb_scan_release(s));}
    else {assert(kind==WDB_LIST);size_t n;OK(wdb_value_len(v,&n));assert(n<=16);printf("l[");
        for(size_t i=0;i<n;i++) {wdb_value *x;if(i)printf(",");OK(wdb_list_get(v,i,&x));value(x);OK(wdb_value_release(x));}printf("]");}
}
static wdb_value *make(wdb_database *db,uint32_t n) {
    wdb_value *u,*b,*list,*next,*map,*result;uint8_t bytes[]={0,(uint8_t)n,255};
    OK(wdb_value_make(db,WDB_UNSIGNED,n+1,NULL,0,&u));
    OK(wdb_value_make(db,WDB_BYTES,0,bytes,3,&b));
    OK(wdb_value_make(db,WDB_LIST,0,NULL,0,&list));
    OK(wdb_list_push(list,b,&next));OK(wdb_value_release(list));list=next;
    OK(wdb_list_push(list,u,&next));OK(wdb_value_release(list));list=next;
    OK(wdb_value_release(u));OK(wdb_value_release(b));
    OK(wdb_value_make(db,WDB_MAP,0,NULL,0,&map));
    OK(wdb_value_make(db,WDB_UNSIGNED,n,NULL,0,&u));
    OK(wdb_map_set(map,(const uint8_t *)"n",1,u,&next));OK(wdb_value_release(map));map=next;
    OK(wdb_map_set(map,(const uint8_t *)"v",1,list,&result));
    OK(wdb_value_release(map));OK(wdb_value_release(list));OK(wdb_value_release(u));return result;
}
static uint32_t read32(FILE *f) {uint8_t b[4];assert(fread(b,1,4,f)==4);return (uint32_t)b[0]|((uint32_t)b[1]<<8)|((uint32_t)b[2]<<16)|((uint32_t)b[3]<<24);}
int main(int argc,char **argv) {
    assert(argc==4);FILE *f=fopen(argv[1],"rb");assert(f);char magic[8];assert(fread(magic,1,8,f)==8 && !memcmp(magic,"WDBTRC1\n",8));
    uint32_t count=read32(f);assert(count<=256);uint32_t cells[256][4];
    for(uint32_t i=0;i<count;i++)for(int j=0;j<4;j++)cells[i][j]=read32(f);
    assert(fgetc(f)==EOF);assert(!fclose(f));
    uint8_t identity[256];size_t required;OK(wdb_source_id(identity,sizeof identity,&required));assert(required==strlen(argv[3])&&!memcmp(identity,argv[3],required));
    assert(wdb_abi_version()==WDB_ABI_VERSION && wdb_format_version()==WDB_FORMAT_VERSION);
    wdb_database *db;size_t pathlen=strlen(argv[2]);const uint8_t *path=(const uint8_t *)argv[2];
    OK(wdb_file_create(path,pathlen,&db));uint64_t collection;OK(wdb_collection_create(db,&collection));assert(collection==1);
    wdb_tx *tx[4]={0};wdb_snapshot *held=NULL;
    for(uint32_t i=0;i<count;i++) {
        uint32_t op=cells[i][0],slot=cells[i][1],k=cells[i][2],n=cells[i][3];
        assert(op>=1&&op<=13&&slot<4&&k<=255&&n<=254);uint8_t key=(uint8_t)k;
        int status=0;uint64_t result=UINT64_C(0xdeadbeefcafebabe);wdb_value *got=NULL;wdb_scan *got_scan=NULL;
        switch(op) {
        case 1: assert(!tx[slot]);status=wdb_tx_begin(db,WDB_RECORDS,&tx[slot]);break;
        case 2: {assert(tx[slot]);wdb_value *v=make(db,n);status=wdb_tx_put(tx[slot],collection,&key,1,v);OK(wdb_value_release(v));break;}
        case 3: {
            assert(tx[slot]);wdb_value *probe=NULL;
            OK(wdb_value_make(db,WDB_UNSIGNED,123,NULL,0,&probe));got=probe;
            status=wdb_tx_get(tx[slot],collection,&key,1,&got);
            if(status) { assert(got==NULL); } else { assert(got!=NULL && got!=probe); }
            OK(wdb_value_release(probe));break;
        }
        case 4: assert(tx[slot]);status=wdb_tx_scan(tx[slot],collection,&got_scan);break;
        case 5: assert(tx[slot]);status=wdb_tx_commit(tx[slot],&result);if(status)assert(result==0);break;
        case 6: assert(tx[slot]);status=wdb_tx_delete(tx[slot],collection,&key,1);break;
        case 7: assert(tx[slot]);status=wdb_tx_release(tx[slot]);tx[slot]=NULL;break;
        case 8: assert(!held);status=wdb_snapshot_acquire(db,&held);break;
        case 9: assert(held);status=wdb_snapshot_release(held);held=NULL;break;
        case 10: assert(held);status=wdb_snapshot_retain(db,held,&result);break;
        case 11: {
            assert(!held);wdb_snapshot *probe=NULL;OK(wdb_snapshot_acquire(db,&probe));held=probe;
            status=wdb_retained_open(db,k,&held);
            if(status) { assert(held==NULL); } else { assert(held!=NULL && held!=probe); }
            OK(wdb_snapshot_release(probe));break;
        }
        case 12: assert(!held);for(int j=0;j<4;j++)assert(!tx[j]);OK(wdb_close(db));status=wdb_file_open(path,pathlen,&db);break;
        case 13: status=wdb_retained_release(db,k);break;
        }
        wdb_snapshot *current;uint64_t revision;wdb_scan *s;
        OK(wdb_snapshot_acquire(db,&current));OK(wdb_snapshot_revision(current,&revision));
        printf("%u %u %d %llu ",i,op,status,(unsigned long long)revision);
        OK(wdb_snapshot_scan(current,collection,&s));OK(wdb_snapshot_release(current));scan(s,0);OK(wdb_scan_release(s));
        printf(" H=");if(held) {OK(wdb_snapshot_revision(held,&revision));printf("%llu",(unsigned long long)revision);OK(wdb_snapshot_scan(held,collection,&s));scan(s,0);OK(wdb_scan_release(s));}else printf("-");
        uint64_t ids[16];size_t len;int probe=wdb_retained_ids(db,NULL,0,&len);assert((probe==WDB_BUFFER&&len>0)||(probe==WDB_OK&&len==0));assert(len<=16);
        OK(wdb_retained_ids(db,ids,16,&len));printf(" R=");if(!len)printf("-");for(size_t j=0;j<len;j++)printf("%s%llu",j?",":"",(unsigned long long)ids[j]);
        printf(" X=");if(got){value(got);OK(wdb_value_release(got));}else if(got_scan){scan(got_scan,0);OK(wdb_scan_release(got_scan));}else if(!status&&(op==5||op==10))printf("%llu",(unsigned long long)result);else printf("-");printf("\n");
    }
    assert(!held);for(int j=0;j<4;j++)assert(!tx[j]);OK(wdb_close(db));return 0;
}
