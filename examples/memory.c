#include "weekend_db.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static wdb_value *integer(wdb_database *db, uint64_t n) {
    wdb_value *out = NULL;
    assert(wdb_value_make(db, WDB_INTEGER, n, NULL, 0, &out) == WDB_OK);
    return out;
}
static uint64_t scalar(wdb_value *value) {
    uint64_t out;
    assert(wdb_value_scalar(value, &out) == WDB_OK);
    return out;
}
int main(void) {
    wdb_database *db;
    uint64_t collection, revision;
    wdb_tx *seed, *first, *second;
    wdb_snapshot *historical, *current;
    wdb_value *zero, *one, *two, *value = NULL;
    assert(wdb_open(&db) == WDB_OK);
    assert(wdb_collection_create(db, &collection) == WDB_OK);
    zero=integer(db,0); one=integer(db,1); two=integer(db,2);
    assert(wdb_tx_begin(db,WDB_RECORDS,&seed) == WDB_OK);
    assert(wdb_tx_put(seed,collection,(const uint8_t *)"a",1,zero) == WDB_OK);
    assert(wdb_tx_put(seed,collection,(const uint8_t *)"b",1,zero) == WDB_OK);
    assert(wdb_tx_commit(seed,&revision) == WDB_OK);
    assert(wdb_tx_release(seed) == WDB_OK);
    assert(wdb_snapshot_acquire(db,&historical) == WDB_OK);
    assert(wdb_close(db) == WDB_BUSY);
    assert(wdb_tx_begin(db,WDB_RECORDS,&first) == WDB_OK);
    assert(wdb_tx_begin(db,WDB_RECORDS,&second) == WDB_OK);
    assert(wdb_tx_put(first,collection,(const uint8_t *)"a",1,one) == WDB_OK);
    assert(wdb_tx_put(second,collection,(const uint8_t *)"b",1,two) == WDB_OK);
    assert(wdb_tx_commit(first,&revision) == WDB_OK);
    assert(wdb_tx_commit(second,&revision) == WDB_OK);
    assert(wdb_tx_release(first) == WDB_OK);
    assert(wdb_tx_release(second) == WDB_OK);
    assert(wdb_snapshot_get(historical,collection,(const uint8_t *)"a",1,&value) == WDB_OK);
    assert(scalar(value) == 0); assert(wdb_value_release(value) == WDB_OK);
    assert(wdb_snapshot_acquire(db,&current) == WDB_OK);
    assert(wdb_snapshot_get(current,collection,(const uint8_t *)"a",1,&value) == WDB_OK);
    assert(scalar(value) == 1); assert(wdb_value_release(value) == WDB_OK);

    assert(wdb_tx_begin(db,WDB_RECORDS,&first) == WDB_OK);
    assert(wdb_tx_begin(db,WDB_RECORDS,&second) == WDB_OK);
    assert(wdb_tx_get(first,collection,(const uint8_t *)"a",1,&value) == WDB_OK);
    assert(wdb_value_release(value) == WDB_OK);
    assert(wdb_tx_put(first,collection,(const uint8_t *)"b",1,one) == WDB_OK);
    assert(wdb_tx_put(second,collection,(const uint8_t *)"a",1,two) == WDB_OK);
    assert(wdb_tx_commit(second,&revision) == WDB_OK);
    assert(wdb_tx_commit(first,&revision) == WDB_CONFLICT);
    assert(wdb_tx_commit(first,&revision) == WDB_CLOSED);
    assert(wdb_tx_release(first) == WDB_OK); assert(wdb_tx_release(second) == WDB_OK);

    wdb_scan *scan;
    size_t count, required;
    uint8_t buffer[8];
    assert(wdb_snapshot_scan(current,collection,&scan) == WDB_OK);
    assert(wdb_snapshot_release(current) == WDB_OK);
    assert(wdb_scan_len(scan,&count) == WDB_OK && count == 2);
    assert(wdb_scan_key(scan,0,NULL,0,&required) == WDB_BUFFER && required == 1);
    assert(wdb_scan_key(scan,0,buffer,sizeof(buffer),&required) == WDB_OK && buffer[0] == 'a');
    assert(wdb_scan_value(scan,1,&value) == WDB_OK && scalar(value) == 2);
    assert(wdb_value_release(value) == WDB_OK); assert(wdb_scan_release(scan) == WDB_OK);
    assert(wdb_tx_begin(db,WDB_RECORDS,&first) == WDB_OK);
    assert(wdb_tx_delete(first,collection,(const uint8_t *)"b",1) == WDB_OK);
    assert(wdb_tx_scan(first,collection,&scan) == WDB_OK);
    assert(wdb_scan_len(scan,&count) == WDB_OK && count == 1);
    assert(wdb_tx_abort(first) == WDB_OK); assert(wdb_tx_release(first) == WDB_OK);
    assert(wdb_scan_value(scan,0,&value) == WDB_OK && scalar(value) == 2);
    assert(wdb_value_release(value) == WDB_OK); assert(wdb_scan_release(scan) == WDB_OK);

    wdb_value *bytes, *map, *list, *pushed, *record, *clone, *edited;
    const uint8_t binary[]={'x',0,'y'};
    assert(wdb_value_make(db,WDB_BYTES,0,binary,3,&bytes) == WDB_OK);
    assert(wdb_value_make(db,WDB_MAP,0,NULL,0,&map) == WDB_OK);
    assert(wdb_value_make(db,WDB_LIST,0,NULL,0,&list) == WDB_OK);
    assert(wdb_list_push(list,bytes,&pushed) == WDB_OK);
    assert(wdb_map_set(map,(const uint8_t *)"tags",4,pushed,&record) == WDB_OK);
    assert(wdb_value_clone(record,&clone) == WDB_OK);
    assert(wdb_map_remove(record,(const uint8_t *)"tags",4,&edited) == WDB_OK);
    value=(wdb_value *)(uintptr_t)1;
    assert(wdb_map_get(edited,(const uint8_t *)"tags",4,&value) == WDB_NOT_FOUND && value == NULL);
    assert(wdb_value_release(edited) == WDB_OK);
    assert(wdb_map_get(clone,(const uint8_t *)"tags",4,&value) == WDB_OK);
    assert(wdb_value_release(value) == WDB_OK);
    assert(wdb_list_insert(pushed,0,one,&edited) == WDB_OK);
    assert(wdb_list_get(edited,0,&value) == WDB_OK && scalar(value) == 1);
    assert(wdb_value_release(value) == WDB_OK);
    assert(wdb_list_remove(edited,0,&value) == WDB_OK);
    assert(wdb_value_release(edited) == WDB_OK); edited=value;
    assert(wdb_list_set(edited,0,two,&value) == WDB_OK);
    assert(wdb_value_release(value) == WDB_OK); assert(wdb_value_release(edited) == WDB_OK);
    assert(wdb_value_bytes(bytes,NULL,0,&required) == WDB_BUFFER && required == 3);
    memset(buffer,0xff,sizeof(buffer));
    assert(wdb_value_bytes(bytes,buffer,2,&required) == WDB_BUFFER && buffer[0] == 0xff);
    assert(wdb_value_bytes(bytes,buffer,sizeof(buffer),&required) == WDB_OK && !memcmp(binary,buffer,3));
    int32_t kind;
    assert(wdb_value_kind(bytes,&kind) == WDB_OK && kind == WDB_BYTES);
    assert(wdb_value_len(pushed,&count) == WDB_OK && count == 1);
    assert(wdb_map_scan(clone,&scan) == WDB_OK);
    assert(wdb_scan_len(scan,&count) == WDB_OK && count == 1);
    assert(wdb_scan_release(scan) == WDB_OK);
    assert(wdb_value_make(db,WDB_BYTES,0,binary,1024u*1024u+1,&value) == WDB_LIMIT && value == NULL);
    wdb_database *other; wdb_value *foreign;
    assert(wdb_open(&other) == WDB_OK); foreign=integer(other,7);
    assert(wdb_list_push(list,foreign,&value) == WDB_FOREIGN_STORE && value == NULL);
    assert(wdb_value_release(foreign) == WDB_OK); assert(wdb_close(other) == WDB_OK);
    assert(wdb_value_release(bytes) == WDB_OK); assert(wdb_value_release(map) == WDB_OK);
    assert(wdb_value_release(list) == WDB_OK); assert(wdb_value_release(pushed) == WDB_OK);
    assert(wdb_value_release(record) == WDB_OK); assert(wdb_value_release(clone) == WDB_OK);
    assert(wdb_value_release(zero) == WDB_OK); assert(wdb_value_release(one) == WDB_OK); assert(wdb_value_release(two) == WDB_OK);
    assert(wdb_collection_drop(db,collection) == WDB_OK);
    assert(wdb_snapshot_revision(historical,&revision) == WDB_OK && revision > 0);
    assert(wdb_snapshot_get(historical,collection,(const uint8_t *)"a",1,&value) == WDB_OK && scalar(value) == 0);
    assert(wdb_value_release(value) == WDB_OK);
    assert(wdb_snapshot_release(historical) == WDB_OK);
    assert(wdb_value_release(NULL) == WDB_OK);
    assert(wdb_close(db) == WDB_OK);
    puts("C shared-library engine: snapshots, merges, conflicts, values and cleanup passed.");
    return 0;
}
