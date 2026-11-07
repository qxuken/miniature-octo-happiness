#include "weekend_probe.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static void expect_bytes(wp_value *value, const uint8_t *bytes, size_t len) {
    uint8_t buffer[32] = {0};
    size_t required = 999;
    assert(wp_read_bytes(value, NULL, 0, &required) == (len ? WP_BUFFER : WP_OK));
    assert(required == len);
    assert(wp_read_bytes(value, buffer, sizeof(buffer), &required) == WP_OK);
    assert(required == len && !memcmp(buffer, bytes, len));
}

/* Separate translation unit also exercises declaration-only header inclusion. */
int wp_contract(void) {
    wp_db *db = NULL, *other = NULL;
    wp_value *bytes = NULL, *list = NULL, *tags = NULL, *map = NULL;
    wp_value *old = NULL, *clone = NULL, *more = NULL, *edited = NULL;
    wp_value *got = NULL, *item = NULL, *bad = NULL, *foreign = NULL;
    uint8_t source[] = {'a', 0, 'b'};
    const uint8_t original[] = {'a', 0, 'b'};
    const uint8_t key[] = {'t', 0, 'g'};
    assert(wp_open(&db) == WP_OK);
    assert(wp_make(db, WP_BYTES, source, sizeof(source), &bytes) == WP_OK);
    source[0] = 'z'; /* Inputs were copied. */
    assert(wp_make(db, WP_LIST, NULL, 0, &list) == WP_OK);
    assert(wp_list_push(list, bytes, &tags) == WP_OK);
    assert(wp_make(db, WP_MAP, NULL, 0, &map) == WP_OK);
    assert(wp_map_set(map, key, sizeof(key), tags, &old) == WP_OK);
    assert(wp_clone(old, &clone) == WP_OK);
    assert(wp_list_push(tags, bytes, &more) == WP_OK);
    assert(wp_map_set(clone, key, sizeof(key), more, &edited) == WP_OK);
    assert(wp_close(db) == WP_BUSY);

    wp_release(old); wp_release(tags); wp_release(more);
    wp_release(bytes); wp_release(list); wp_release(map);
    /* The clone and edited graph own their shared children. */
    assert(wp_map_get(clone, key, sizeof(key), &got) == WP_OK);
    assert(wp_list_get(got, 1, &bad) == WP_BOUNDS && bad == NULL);
    assert(wp_list_get(got, 0, &item) == WP_OK);
    expect_bytes(item, original, sizeof(original));
    wp_release(got);
    assert(wp_map_get(edited, key, sizeof(key), &got) == WP_OK);
    assert(wp_list_get(got, 1, &bad) == WP_OK);
    expect_bytes(bad, original, sizeof(original));
    wp_release(bad); wp_release(got);

    assert(wp_map_get(clone, NULL, 0, &bad) == WP_NOT_FOUND && bad == NULL);
    assert(wp_map_get(item, key, sizeof(key), &bad) == WP_TYPE && bad == NULL);
    assert(wp_list_get(NULL, 0, &bad) == WP_INVALID && bad == NULL);
    assert(wp_make(db, 99, NULL, 0, &bad) == WP_INVALID && bad == NULL);
    assert(wp_make(db, WP_BYTES, NULL, 1, &bad) == WP_INVALID && bad == NULL);
    assert(wp_make(db, WP_BYTES, source, 1024u * 1024u + 1, &bad) == WP_LIMIT);
    assert(wp_open(&other) == WP_OK);
    assert(wp_make(other, WP_BYTES, NULL, 0, &foreign) == WP_OK);
    assert(wp_map_set(clone, key, sizeof(key), foreign, &bad) == WP_INVALID);
    wp_release(foreign);
    assert(wp_close(other) == WP_OK);

    /* Empty bytes and empty key are valid and separate from absence. */
    assert(wp_make(db, WP_BYTES, NULL, 0, &bytes) == WP_OK);
    expect_bytes(bytes, original, 0);
    assert(wp_map_set(clone, NULL, 0, bytes, &map) == WP_OK);
    assert(wp_map_get(map, NULL, 0, &got) == WP_OK);
    expect_bytes(got, original, 0);
    wp_release(got); wp_release(map); wp_release(bytes);
    wp_release(item); wp_release(clone); wp_release(edited);

    /* Enforced depth bound also bounds recursive destruction. */
    assert(wp_make(db, WP_LIST, NULL, 0, &list) == WP_OK);
    assert(wp_clone(list, &got) == WP_OK);
    for (size_t depth = 1; depth < 64; depth++) {
        assert(wp_list_push(list, got, &more) == WP_OK);
        wp_release(got); got = more;
    }
    assert(wp_list_push(list, got, &bad) == WP_LIMIT && bad == NULL);
    wp_release(got); wp_release(list);
    wp_release(NULL);
    assert(wp_close(db) == WP_OK);
    puts("C ABI contract: ok");
    return 0;
}
