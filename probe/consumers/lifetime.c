#include "weekend_probe.h"
#include <assert.h>

/* Deliberately invalid caller, run only as an expected ASan failure. */
int main(void) {
    wp_db *db;
    wp_value *value;
    assert(wp_open(&db) == WP_OK);
    assert(wp_make(db, WP_BYTES, (const uint8_t *)"expired", 7, &value) == WP_OK);
    wp_release(value);
    size_t required;
    (void)wp_read_bytes(value, NULL, 0, &required);
    return 0;
}
