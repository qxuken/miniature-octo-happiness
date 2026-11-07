#include "weekend_db.h"
#include <assert.h>
#include <pthread.h>
#include <stdio.h>

#define OK(call) assert((call) == WDB_OK)
enum { WRITERS = 4, ROUNDS = 100 };
static wdb_database *db;
static wdb_snapshot *pinned;
static wdb_value *shared;
static uint64_t collection;
static pthread_mutex_t mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t condition = PTHREAD_COND_INITIALIZER;
static unsigned prepared;

static uint64_t read_number(const wdb_snapshot *snapshot, uint8_t writer, uint8_t side) {
    uint8_t key[] = {writer, side};
    wdb_value *v = NULL;
    uint64_t bits = 0;
    OK(wdb_snapshot_get(snapshot, collection, key, sizeof key, &v));
    OK(wdb_value_scalar(v, &bits));
    OK(wdb_value_release(v));
    return bits;
}
static void *writer(void *arg) {
    uint8_t id = *(uint8_t *)arg;
    for (unsigned n = 1; n <= ROUNDS; ++n) {
        wdb_tx *tx = NULL;
        wdb_value *v = NULL, *clone = NULL;
        uint64_t revision = 0;
        OK(wdb_tx_begin(db, WDB_RECORDS, &tx));
        OK(wdb_value_clone(shared, &clone));
        OK(wdb_value_make(db, WDB_UNSIGNED, n, NULL, 0, &v));
        for (uint8_t side = 0; side < 2; ++side) {
            uint8_t key[] = {id, side};
            OK(wdb_tx_put(tx, collection, key, sizeof key, v));
        }
        if (n == 1) {
            assert(pthread_mutex_lock(&mutex) == 0);
            ++prepared;
            assert(pthread_cond_broadcast(&condition) == 0);
            while (prepared != WRITERS)
                assert(pthread_cond_wait(&condition, &mutex) == 0);
            assert(pthread_mutex_unlock(&mutex) == 0);
        }
        OK(wdb_tx_commit(tx, &revision));
        OK(wdb_tx_release(tx));
        OK(wdb_value_release(v));
        OK(wdb_value_release(clone));
        assert(read_number(pinned, id, 0) == 0);
    }
    return NULL;
}
static void *reader(void *arg) {
    (void)arg;
    for (unsigned n = 0; n < 2000; ++n) {
        wdb_snapshot *s = NULL;
        wdb_scan *scan = NULL;
        size_t len = 0;
        OK(wdb_snapshot_acquire(db, &s));
        for (uint8_t i = 0; i < WRITERS; ++i) {
            assert(read_number(s, i, 0) == read_number(s, i, 1));
            assert(read_number(pinned, i, 1) == 0);
        }
        OK(wdb_snapshot_scan(s, collection, &scan));
        OK(wdb_scan_len(scan, &len));
        assert(len == 2 * WRITERS);
        OK(wdb_scan_release(scan));
        OK(wdb_snapshot_release(s));
    }
    return NULL;
}
int main(void) {
    OK(wdb_open(&db));
    OK(wdb_collection_create(db, &collection));
    OK(wdb_value_make(db, WDB_UNSIGNED, 0, NULL, 0, &shared));
    wdb_tx *tx = NULL;
    uint64_t revision = 0;
    OK(wdb_tx_begin(db, WDB_RECORDS, &tx));
    for (uint8_t i = 0; i < WRITERS; ++i) {
        for (uint8_t side = 0; side < 2; ++side) {
            uint8_t key[] = {i, side};
            OK(wdb_tx_put(tx, collection, key, sizeof key, shared));
        }
    }
    OK(wdb_tx_commit(tx, &revision));
    OK(wdb_tx_release(tx));
    OK(wdb_snapshot_acquire(db, &pinned));
    pthread_t writers[WRITERS], readers[2];
    uint8_t ids[WRITERS];
    for (uint8_t i = 0; i < WRITERS; ++i) {
        ids[i] = i;
        assert(pthread_create(&writers[i], NULL, writer, &ids[i]) == 0);
    }
    for (unsigned i = 0; i < 2; ++i)
        assert(pthread_create(&readers[i], NULL, reader, NULL) == 0);
    for (unsigned i = 0; i < WRITERS; ++i)
        assert(pthread_join(writers[i], NULL) == 0);
    for (unsigned i = 0; i < 2; ++i)
        assert(pthread_join(readers[i], NULL) == 0);
    /* Close is quiescent here. Live handles still prevent destruction. */
    assert(wdb_close(db) == WDB_BUSY);
    wdb_snapshot *current = NULL;
    OK(wdb_snapshot_acquire(db, &current));
    for (uint8_t i = 0; i < WRITERS; ++i)
        assert(read_number(current, i, 0) == ROUNDS);
    OK(wdb_snapshot_revision(current, &revision));
    assert(revision == 2 + WRITERS * ROUNDS);
    OK(wdb_snapshot_release(current));
    OK(wdb_snapshot_release(pinned));
    OK(wdb_value_release(shared));
    OK(wdb_close(db));
    assert(pthread_cond_destroy(&condition) == 0);
    assert(pthread_mutex_destroy(&mutex) == 0);
    puts("C concurrent readers/commits passed");
    return 0;
}
