#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

static size_t remaining = (size_t)-1;
static void *fault_calloc(size_t count, size_t size) {
    if (remaining == 0) return NULL;
    if (remaining != (size_t)-1) remaining--;
    return calloc(count, size);
}
#define WP_CALLOC fault_calloc
#include "../c/weekend_probe.c"

int main(void) {
    wp_db *db = NULL;
    remaining = 0;
    assert(wp_open(&db) == WP_OOM && db == NULL);
    remaining = (size_t)-1;
    assert(wp_open(&db) == WP_OK);
    wp_value *map, *list, *bytes, *seed;
    assert(wp_make(db, WP_MAP, NULL, 0, &map) == WP_OK);
    assert(wp_make(db, WP_LIST, NULL, 0, &list) == WP_OK);
    assert(wp_make(db, WP_BYTES, (const uint8_t *)"old", 3, &bytes) == WP_OK);
    assert(wp_map_set(map, (const uint8_t *)"x", 1, bytes, &seed) == WP_OK);
    /* Exhaust every allocation site in each operation, including wrapping. */
    for (int operation = 0; operation < 6; operation++) {
        int succeeded = 0;
        for (size_t fail = 0; fail < 32; fail++) {
            wp_value *out = NULL;
            remaining = fail;
            wp_status status;
            switch (operation) {
            case 0: status = wp_make(db, WP_BYTES, (const uint8_t *)"new", 3, &out); break;
            case 1: status = wp_map_set(seed, (const uint8_t *)"y", 1, bytes, &out); break;
            case 2: status = wp_list_push(list, bytes, &out); break;
            case 3: status = wp_clone(seed, &out); break;
            case 4: status = wp_map_get(seed, (const uint8_t *)"x", 1, &out); break;
            default: status = wp_make(db, WP_LIST, NULL, 0, &out); break;
            }
            remaining = (size_t)-1;
            if (status == WP_OK) { wp_release(out); succeeded = 1; break; }
            assert(status == WP_OOM && out == NULL);
            assert(db->handles == 4);
            wp_value *got;
            uint8_t buffer[3]; size_t len;
            assert(wp_map_get(seed, (const uint8_t *)"x", 1, &got) == WP_OK);
            assert(wp_read_bytes(got, buffer, 3, &len) == WP_OK);
            assert(len == 3 && !memcmp(buffer, "old", 3));
            wp_release(got);
        }
        assert(succeeded);
    }
    wp_release(seed); wp_release(bytes); wp_release(list); wp_release(map);
    assert(wp_close(db) == WP_OK);
    puts("C allocation failures: ok");
    return 0;
}
