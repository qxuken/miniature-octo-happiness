#ifndef WEEKEND_PROBE_H
#define WEEKEND_PROBE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

/* Milestone-1 packaging probe, not the database API. See probe/README.md. */
typedef struct wp_db wp_db;
typedef struct wp_value wp_value;
typedef int32_t wp_status;
enum {
    WP_OK = 0, WP_INVALID = 1, WP_TYPE = 2, WP_NOT_FOUND = 3,
    WP_BOUNDS = 4, WP_BUFFER = 5, WP_BUSY = 6, WP_LIMIT = 7, WP_OOM = 8,
    WP_PANIC = 9
};
enum { WP_BYTES = 1, WP_MAP = 2, WP_LIST = 3 };

wp_status wp_open(wp_db **out);
wp_status wp_close(wp_db *db);
wp_status wp_make(wp_db *db, int32_t kind, const uint8_t *bytes,
                  size_t len, wp_value **out);
wp_status wp_clone(const wp_value *value, wp_value **out);
void wp_release(wp_value *value);
wp_status wp_map_set(const wp_value *map, const uint8_t *key, size_t len,
                     const wp_value *value, wp_value **out);
wp_status wp_map_get(const wp_value *map, const uint8_t *key, size_t len,
                     wp_value **out);
wp_status wp_list_push(const wp_value *list, const wp_value *value, wp_value **out);
wp_status wp_list_get(const wp_value *list, size_t index, wp_value **out);
wp_status wp_read_bytes(const wp_value *value, uint8_t *buffer,
                        size_t capacity, size_t *required);

#ifdef __cplusplus
}
#endif
#endif
