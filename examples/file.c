#include "weekend_db.h"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
int main(int argc,char **argv) {
    assert(argc==2);
    const uint8_t *path=(const uint8_t *)argv[1];size_t len=strlen(argv[1]);
    wdb_database *db,*other=NULL;wdb_tx *tx;wdb_value *one,*two,*got;wdb_snapshot *old,*retained;
    uint64_t collection,revision,id,value,ids[2];size_t required;
    assert(wdb_file_create(path,len,&db)==WDB_OK);
    uint64_t before=9,after=9;
    assert(wdb_file_compact(path,len,&before,&after)==WDB_BUSY && before==0 && after==0);
    assert(wdb_file_open(path,len,&other)==WDB_BUSY && other==NULL);
    assert(wdb_file_create(path,len,&other)==WDB_ALREADY_EXISTS && other==NULL);
    assert(wdb_collection_create(db,&collection)==WDB_OK);
    assert(wdb_value_make(db,WDB_UNSIGNED,1,NULL,0,&one)==WDB_OK);
    assert(wdb_value_make(db,WDB_UNSIGNED,2,NULL,0,&two)==WDB_OK);
    assert(wdb_tx_begin(db,WDB_RECORDS,&tx)==WDB_OK);
    assert(wdb_tx_put(tx,collection,(const uint8_t *)"a",1,one)==WDB_OK);
    assert(wdb_tx_commit(tx,&revision)==WDB_OK);assert(wdb_tx_release(tx)==WDB_OK);
    assert(wdb_snapshot_acquire(db,&old)==WDB_OK);assert(wdb_snapshot_retain(db,old,&id)==WDB_OK);
    assert(wdb_tx_begin(db,WDB_RECORDS,&tx)==WDB_OK);
    assert(wdb_tx_put(tx,collection,(const uint8_t *)"a",1,two)==WDB_OK);
    assert(wdb_tx_commit(tx,&revision)==WDB_OK);assert(wdb_tx_release(tx)==WDB_OK);
    assert(wdb_close(db)==WDB_BUSY);
    assert(wdb_snapshot_release(old)==WDB_OK);assert(wdb_value_release(one)==WDB_OK);assert(wdb_value_release(two)==WDB_OK);
    assert(wdb_close(db)==WDB_OK);
    assert(wdb_file_compact(path,len,&before,&after)==WDB_OK && after<=before);
    assert(wdb_file_open(path,len,&db)==WDB_OK);
    assert(wdb_retained_ids(db,NULL,0,&required)==WDB_BUFFER && required==1);
    assert(wdb_retained_ids(db,ids,2,&required)==WDB_OK && required==1 && ids[0]==id);
    assert(wdb_retained_open(db,id,&retained)==WDB_OK);
    assert(wdb_snapshot_get(retained,collection,(const uint8_t *)"a",1,&got)==WDB_OK);
    assert(wdb_value_scalar(got,&value)==WDB_OK && value==1);assert(wdb_value_release(got)==WDB_OK);
    assert(wdb_snapshot_acquire(db,&old)==WDB_OK);
    assert(wdb_snapshot_get(old,collection,(const uint8_t *)"a",1,&got)==WDB_OK);
    assert(wdb_value_scalar(got,&value)==WDB_OK && value==2);assert(wdb_value_release(got)==WDB_OK);
    assert(wdb_retained_release(db,id)==WDB_OK);
    assert(wdb_snapshot_get(retained,collection,(const uint8_t *)"a",1,&got)==WDB_OK);
    assert(wdb_value_scalar(got,&value)==WDB_OK && value==1);assert(wdb_value_release(got)==WDB_OK);
    assert(wdb_snapshot_release(old)==WDB_OK);assert(wdb_snapshot_release(retained)==WDB_OK);assert(wdb_close(db)==WDB_OK);
    assert(wdb_file_open(path,len,&db)==WDB_OK);
    assert(wdb_retained_ids(db,NULL,0,&required)==WDB_OK && required==0);
    assert(wdb_retained_open(db,id,&old)==WDB_NOT_FOUND && old==NULL);assert(wdb_close(db)==WDB_OK);
    puts("C file/shared-library consumer: durable retention, reopen, release and close passed.");return 0;
}
