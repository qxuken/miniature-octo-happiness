#ifndef WEEKEND_DB_H
#define WEEKEND_DB_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

/* Experimental API. See DESIGN.md for file and in-memory contracts.
 * Database and immutable value/snapshot/scan handles may be shared across
 * threads. Transactions require exclusive access; synchronized transfer is OK.
 * Release each returned handle once, with no concurrent use of that handle.
 * Close requires external quiescence: no calls on the database or its handles
 * may overlap close, including acquisition/release. BUSY is a lifetime check,
 * not a protocol for racing close against new calls. Callers arrange transfer,
 * handle lifetime, output-buffer synchronization, and database quiescence.
 * Close returns WDB_BUSY while any handle remains. Null releases are harmless.
 * Slices are borrowed for the call and copied when stored; embedded NULs work.
 * Output bytes are copied to caller buffers. BUFFER returns required length
 * without partial copying. Output handle pointers are reset to NULL on errors.
 * Non-null pointers must be live, aligned, valid for the stated length/type;
 * output storage must not alias inputs. Released/forged handles are invalid.
 */
typedef struct wdb_database wdb_database;
typedef struct wdb_value wdb_value;
typedef struct wdb_snapshot wdb_snapshot;
typedef struct wdb_tx wdb_tx;
typedef struct wdb_scan wdb_scan;
typedef int32_t wdb_status;
enum {
    WDB_OK=0, WDB_INVALID=1, WDB_TYPE=2, WDB_NOT_FOUND=3, WDB_BOUNDS=4,
    WDB_BUFFER=5, WDB_BUSY=6, WDB_LIMIT=7, WDB_OUT_OF_MEMORY=8, WDB_PANIC=9,
    WDB_CONFLICT=10, WDB_CLOSED=11, WDB_FOREIGN_STORE=12, WDB_EXHAUSTED=13,
    WDB_IO=14, WDB_OUTCOME_UNKNOWN=15, WDB_RECOVERY_REQUIRED=16,
    WDB_CORRUPT=17, WDB_UNSUPPORTED_FORMAT=18, WDB_ALREADY_EXISTS=19,
    WDB_UNSUPPORTED_PLATFORM=20
};
enum { WDB_NONE=0, WDB_BOOLEAN=1, WDB_INTEGER=2, WDB_UNSIGNED=3,
       WDB_FLOAT=4, WDB_BYTES=5, WDB_MAP=6, WDB_LIST=7 };
enum { WDB_RECORDS=0, WDB_WHOLE_DATABASE=1 };
#define WDB_ABI_VERSION 1u
#define WDB_FORMAT_VERSION 2u
/* These queries require no database. Strings are UTF-8 bytes without a NUL.
 * The ordinary required-size and BUFFER rules apply. Source ID is "unknown"
 * unless the build sets WEEKEND_DB_SOURCE_ID. Feature bits are replication=1,
 * CRDT=2, fault injection=4. */
uint32_t wdb_abi_version(void);
uint32_t wdb_format_version(void);
uint32_t wdb_features(void);
wdb_status wdb_package_version(uint8_t *buffer, size_t capacity, size_t *required);
wdb_status wdb_source_id(uint8_t *buffer, size_t capacity, size_t *required);
wdb_status wdb_open(wdb_database **out);
/* Explicit-length native path bytes on macOS and Linux x86_64/aarch64, UTF-8
 * on Windows (local fixed NTFS). Embedded NULs are invalid. Creation is
 * exclusive and never overwrites. Open is read/write with an exclusive lease.
 * Other platforms return UNSUPPORTED_PLATFORM. */
wdb_status wdb_file_create(const uint8_t *path, size_t len, wdb_database **out);
wdb_status wdb_file_open(const uint8_t *path, size_t len, wdb_database **out);
/* Offline macOS/Linux compaction: close ALL database handles for this path first.
 * BUSY if an inode is leased; hard-link sources are rejected. Current/retained
 * roots, revisions and local IDs survive; file format remains v2. A verified,
 * durable sibling copy is atomically exchanged before the original is removed.
 * Errors may leave .wdb-compact-*.tmp siblings: never auto-delete recovery copies.
 * OUTCOME_UNKNOWN from the exchange phase onward requires reopening the canonical
 * path. Output counts are data-fork lengths; with valid output pointers both
 * reset to zero on error.
 * Unsupported platforms return UNSUPPORTED_PLATFORM. Outputs cannot alias.
 */
wdb_status wdb_file_compact(const uint8_t *path, size_t len, uint64_t *before, uint64_t *after);
wdb_status wdb_close(wdb_database *db);
/* IDs are local, monotonic, and persist on file stores. Releasing retention
 * does not release/invalidate live snapshot handles or immediately reclaim space.
 * ids capacity/required are element counts, not byte lengths. */
wdb_status wdb_snapshot_retain(wdb_database *db, const wdb_snapshot *snapshot, uint64_t *out);
wdb_status wdb_retained_release(wdb_database *db, uint64_t id);
wdb_status wdb_retained_open(wdb_database *db, uint64_t id, wdb_snapshot **out);
wdb_status wdb_retained_ids(wdb_database *db, uint64_t *buffer, size_t capacity, size_t *required);
/* Catalog create/drop are immediate atomic commits. IDs are local to this
 * database and never reused. The same integer in another database is unrelated. */
wdb_status wdb_collection_create(wdb_database *db, uint64_t *out);
wdb_status wdb_collection_drop(wdb_database *db, uint64_t id);
/* Scalar bits: integer two's complement; float IEEE-754 binary64 bit pattern;
 * bool 0/1; none/map/list require 0. Only BYTES takes a nonzero slice length. */
wdb_status wdb_value_make(wdb_database *db, int32_t kind, uint64_t bits,
                           const uint8_t *bytes, size_t len, wdb_value **out);
wdb_status wdb_value_clone(const wdb_value *value, wdb_value **out);
wdb_status wdb_value_release(wdb_value *value);
wdb_status wdb_value_kind(const wdb_value *value, int32_t *out);
wdb_status wdb_value_scalar(const wdb_value *value, uint64_t *out);
wdb_status wdb_value_len(const wdb_value *value, size_t *out);
wdb_status wdb_value_bytes(const wdb_value *value, uint8_t *buffer, size_t capacity, size_t *required);
wdb_status wdb_map_set(const wdb_value *map, const uint8_t *key, size_t len, const wdb_value *value, wdb_value **out);
wdb_status wdb_map_get(const wdb_value *map, const uint8_t *key, size_t len, wdb_value **out);
wdb_status wdb_map_remove(const wdb_value *map, const uint8_t *key, size_t len, wdb_value **out);
wdb_status wdb_map_scan(const wdb_value *map, wdb_scan **out);
wdb_status wdb_list_push(const wdb_value *list, const wdb_value *value, wdb_value **out);
wdb_status wdb_list_set(const wdb_value *list, size_t index, const wdb_value *value, wdb_value **out);
wdb_status wdb_list_insert(const wdb_value *list, size_t index, const wdb_value *value, wdb_value **out);
wdb_status wdb_list_remove(const wdb_value *list, size_t index, wdb_value **out);
wdb_status wdb_list_get(const wdb_value *list, size_t index, wdb_value **out);
wdb_status wdb_snapshot_acquire(wdb_database *db, wdb_snapshot **out);
wdb_status wdb_snapshot_release(wdb_snapshot *snapshot);
wdb_status wdb_snapshot_revision(const wdb_snapshot *snapshot, uint64_t *out);
wdb_status wdb_snapshot_get(const wdb_snapshot *snapshot, uint64_t id, const uint8_t *key, size_t len, wdb_value **out);
wdb_status wdb_snapshot_scan(const wdb_snapshot *snapshot, uint64_t id, wdb_scan **out);
wdb_status wdb_tx_begin(wdb_database *db, int32_t validation, wdb_tx **out);
wdb_status wdb_tx_release(wdb_tx *tx);
wdb_status wdb_tx_get(wdb_tx *tx, uint64_t id, const uint8_t *key, size_t len, wdb_value **out);
wdb_status wdb_tx_put(wdb_tx *tx, uint64_t id, const uint8_t *key, size_t len, const wdb_value *value);
wdb_status wdb_tx_delete(wdb_tx *tx, uint64_t id, const uint8_t *key, size_t len);
wdb_status wdb_tx_scan(wdb_tx *tx, uint64_t id, wdb_scan **out);
/* Commit errors are terminal: CONFLICT/IO abort; preparation errors fail.
 * OUTCOME_UNKNOWN requires reopening to determine publication; IO and
 * OUTCOME_UNKNOWN both block further writes. Release terminal handles as usual. */
wdb_status wdb_tx_commit(wdb_tx *tx, uint64_t *revision);
wdb_status wdb_tx_abort(wdb_tx *tx);
/* Materialized scans own their keys and values and remain valid independently
 * of their source handle. Record keys are sorted in binary lexicographic order. */
wdb_status wdb_scan_release(wdb_scan *scan);
wdb_status wdb_scan_len(const wdb_scan *scan, size_t *out);
wdb_status wdb_scan_key(const wdb_scan *scan, size_t index, uint8_t *buffer, size_t capacity, size_t *required);
wdb_status wdb_scan_value(const wdb_scan *scan, size_t index, wdb_value **out);

#ifdef __cplusplus
}
#endif
#endif
