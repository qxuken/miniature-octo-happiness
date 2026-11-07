#include "weekend_probe.h"
#include <stdlib.h>
#include <string.h>

#ifndef WP_CALLOC
#define WP_CALLOC calloc
#endif

#define WP_MAX_BYTES (1024u * 1024u)
#define WP_MAX_ENTRIES 4096u
#define WP_MAX_DEPTH 64u

typedef struct wp_node wp_node;
typedef struct { uint8_t *key; size_t len; wp_node *value; } wp_entry;
struct wp_node {
    size_t refs, len, depth;
    int32_t kind;
    union { uint8_t *bytes; wp_entry *map; wp_node **list; } data;
};
struct wp_db { size_t handles; };
struct wp_value { wp_db *owner; wp_node *node; };

static void *wp_alloc(size_t count, size_t size) {
    if (size && count > SIZE_MAX / size) return NULL;
    return WP_CALLOC(count ? count : 1, size);
}
static uint8_t *wp_copy(const uint8_t *bytes, size_t len) {
    uint8_t *copy = wp_alloc(len, 1);
    if (copy && len) memcpy(copy, bytes, len);
    return copy;
}
static wp_node *wp_new(int32_t kind) {
    wp_node *node = wp_alloc(1, sizeof(*node));
    if (node) { node->refs = 1; node->kind = kind; node->depth = 1; }
    return node;
}
static int wp_ref(wp_node *node) {
    if (node->refs == SIZE_MAX) return 0;
    node->refs++;
    return 1;
}
static void wp_drop(wp_node *node) {
    if (!node || --node->refs) return;
    if (node->kind == WP_BYTES) free(node->data.bytes);
    if (node->kind == WP_MAP) {
        for (size_t i = 0; i < node->len; i++) {
            free(node->data.map[i].key);
            wp_drop(node->data.map[i].value);
        }
        free(node->data.map);
    }
    if (node->kind == WP_LIST) {
        for (size_t i = 0; i < node->len; i++) wp_drop(node->data.list[i]);
        free(node->data.list);
    }
    free(node);
}
/* Consumes one node reference even when wrapping fails. */
static wp_status wp_wrap(wp_db *db, wp_node *node, wp_value **out) {
    if (!node) return WP_OOM;
    if (db->handles == SIZE_MAX) { wp_drop(node); return WP_LIMIT; }
    wp_value *value = wp_alloc(1, sizeof(*value));
    if (!value) { wp_drop(node); return WP_OOM; }
    value->owner = db; value->node = node; db->handles++;
    *out = value;
    return WP_OK;
}
static int wp_slice(const uint8_t *bytes, size_t len) { return bytes || !len; }
static size_t wp_find(const wp_node *map, const uint8_t *key, size_t len) {
    for (size_t i = 0; i < map->len; i++) {
        const wp_entry *entry = &map->data.map[i];
        if (entry->len == len && (!len || !memcmp(entry->key, key, len))) return i;
    }
    return map->len;
}

wp_status wp_open(wp_db **out) {
    if (!out) return WP_INVALID;
    *out = wp_alloc(1, sizeof(wp_db));
    return *out ? WP_OK : WP_OOM;
}
wp_status wp_close(wp_db *db) {
    if (!db) return WP_INVALID;
    if (db->handles) return WP_BUSY;
    free(db);
    return WP_OK;
}
wp_status wp_make(wp_db *db, int32_t kind, const uint8_t *bytes, size_t len, wp_value **out) {
    if (!out) return WP_INVALID;
    *out = NULL;
    if (!db || !wp_slice(bytes, len) || kind < WP_BYTES || kind > WP_LIST) return WP_INVALID;
    if (len > WP_MAX_BYTES) return WP_LIMIT;
    if (kind != WP_BYTES && len) return WP_INVALID;
    wp_node *node = wp_new(kind);
    if (!node) return WP_OOM;
    if (kind == WP_BYTES) {
        node->data.bytes = wp_copy(bytes, len);
        if (!node->data.bytes) { wp_drop(node); return WP_OOM; }
        node->len = len;
    }
    return wp_wrap(db, node, out);
}
wp_status wp_clone(const wp_value *value, wp_value **out) {
    if (!out) return WP_INVALID;
    *out = NULL;
    if (!value) return WP_INVALID;
    if (!wp_ref(value->node)) return WP_LIMIT;
    return wp_wrap(value->owner, value->node, out);
}
void wp_release(wp_value *value) {
    if (!value) return;
    value->owner->handles--;
    wp_drop(value->node);
    free(value);
}
wp_status wp_map_set(const wp_value *map, const uint8_t *key, size_t len,
                     const wp_value *value, wp_value **out) {
    if (!out) return WP_INVALID;
    *out = NULL;
    if (!map || !value || !wp_slice(key, len) || map->owner != value->owner) return WP_INVALID;
    if (map->node->kind != WP_MAP) return WP_TYPE;
    if (len > WP_MAX_BYTES || value->node->depth >= WP_MAX_DEPTH) return WP_LIMIT;
    size_t index = wp_find(map->node, key, len);
    size_t count = map->node->len + (index == map->node->len);
    if (count > WP_MAX_ENTRIES) return WP_LIMIT;
    wp_node *node = wp_new(WP_MAP);
    if (!node) return WP_OOM;
    node->data.map = wp_alloc(count, sizeof(wp_entry));
    if (!node->data.map) { wp_drop(node); return WP_OOM; }
    for (size_t i = 0; i < count; i++) {
        const uint8_t *source_key = i == index ? key : map->node->data.map[i].key;
        size_t source_len = i == index ? len : map->node->data.map[i].len;
        wp_node *child = i == index ? value->node : map->node->data.map[i].value;
        uint8_t *copy = wp_copy(source_key, source_len);
        if (!copy) { wp_drop(node); return WP_OOM; }
        if (!wp_ref(child)) { free(copy); wp_drop(node); return WP_LIMIT; }
        node->data.map[i] = (wp_entry){copy, source_len, child};
        node->len++;
        if (node->depth < child->depth + 1) node->depth = child->depth + 1;
    }
    return wp_wrap(map->owner, node, out);
}
wp_status wp_map_get(const wp_value *map, const uint8_t *key, size_t len, wp_value **out) {
    if (!out) return WP_INVALID;
    *out = NULL;
    if (!map || !wp_slice(key, len)) return WP_INVALID;
    if (map->node->kind != WP_MAP) return WP_TYPE;
    if (len > WP_MAX_BYTES) return WP_LIMIT;
    size_t index = wp_find(map->node, key, len);
    if (index == map->node->len) return WP_NOT_FOUND;
    wp_node *child = map->node->data.map[index].value;
    if (!wp_ref(child)) return WP_LIMIT;
    return wp_wrap(map->owner, child, out);
}
wp_status wp_list_push(const wp_value *list, const wp_value *value, wp_value **out) {
    if (!out) return WP_INVALID;
    *out = NULL;
    if (!list || !value || list->owner != value->owner) return WP_INVALID;
    if (list->node->kind != WP_LIST) return WP_TYPE;
    if (list->node->len >= WP_MAX_ENTRIES || value->node->depth >= WP_MAX_DEPTH) return WP_LIMIT;
    wp_node *node = wp_new(WP_LIST);
    if (!node) return WP_OOM;
    node->data.list = wp_alloc(list->node->len + 1, sizeof(wp_node *));
    if (!node->data.list) { wp_drop(node); return WP_OOM; }
    node->depth = list->node->depth;
    for (size_t i = 0; i <= list->node->len; i++) {
        wp_node *child = i == list->node->len ? value->node : list->node->data.list[i];
        if (!wp_ref(child)) { wp_drop(node); return WP_LIMIT; }
        node->data.list[node->len++] = child;
    }
    if (node->depth < value->node->depth + 1) node->depth = value->node->depth + 1;
    return wp_wrap(list->owner, node, out);
}
wp_status wp_list_get(const wp_value *list, size_t index, wp_value **out) {
    if (!out) return WP_INVALID;
    *out = NULL;
    if (!list) return WP_INVALID;
    if (list->node->kind != WP_LIST) return WP_TYPE;
    if (index >= list->node->len) return WP_BOUNDS;
    wp_node *child = list->node->data.list[index];
    if (!wp_ref(child)) return WP_LIMIT;
    return wp_wrap(list->owner, child, out);
}
wp_status wp_read_bytes(const wp_value *value, uint8_t *buffer, size_t capacity, size_t *required) {
    if (!required) return WP_INVALID;
    *required = 0;
    if (!value || !wp_slice(buffer, capacity)) return WP_INVALID;
    if (value->node->kind != WP_BYTES) return WP_TYPE;
    *required = value->node->len;
    if (capacity < *required) return WP_BUFFER;
    if (*required) memcpy(buffer, value->node->data.bytes, *required);
    return WP_OK;
}
