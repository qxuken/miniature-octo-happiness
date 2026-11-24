/* The package runner invokes this fixture in a fresh private directory. */
#include <errno.h>

typedef wdb_status (*file_factory)(const uint8_t *, size_t,
                                   const wdb_database_options *, wdb_database **);

static wdb_database_options file_options(const wdb_allocator *callbacks) {
    wdb_database_options options;
    assert(wdb_database_options_init(&options) == WDB_OK);
    options.allocator = callbacks;
    options.limits.max_bytes = 3;
    options.limits.max_live_bytes = 1024u * 1024u;
    return options;
}

static void check_file_factory_rejection(file_factory factory, const char *path) {
    allocator_state state = {0, 0, 0, 0, 0, 0};
    wdb_allocator callbacks = contract_callbacks(&state);
    wdb_database_options options = file_options(&callbacks);
    const uint8_t malformed[] = {'a', 0, 'b'};
    wdb_database *db = (wdb_database *)(uintptr_t)1;
    assert(factory(malformed, sizeof(malformed), &options, &db) == WDB_INVALID && db == NULL);
    assert(factory((const uint8_t *)path, strlen(path), &options, NULL) == WDB_INVALID);
    options.limits.max_depth = 0;
    db = (wdb_database *)(uintptr_t)1;
    assert(factory((const uint8_t *)path, strlen(path), &options, &db) == WDB_INVALID && db == NULL);
    assert(state.requests == 0 && state.retains == 0 && state.releases == 0);
}

static wdb_database *open_allocated_file(file_factory factory, const char *path,
                                         allocator_state *state) {
    wdb_allocator callbacks = contract_callbacks(state);
    wdb_database_options options = file_options(&callbacks);
    wdb_database *db = NULL;
    assert(factory((const uint8_t *)path, strlen(path), &options, &db) == WDB_OK);
    assert(state->requests > 1 && state->retains == 1 && state->releases == 0);
    wdb_limits limits;
    assert(wdb_database_get_limits(db, &limits) == WDB_OK);
    assert(limits.max_bytes == 3 && limits.max_live_bytes == 1024u * 1024u);
    return db;
}

static void file_factory_refusal(file_factory factory, const char *path, size_t fail_at) {
    allocator_state state = {0, 0, fail_at, 0, 0, 0};
    wdb_allocator callbacks = contract_callbacks(&state);
    wdb_database_options options = file_options(&callbacks);
    wdb_database *db = (wdb_database *)(uintptr_t)1;
    assert(factory((const uint8_t *)path, strlen(path), &options, &db) == WDB_OUT_OF_MEMORY);
    assert(db == NULL && state.requests == fail_at && state.live == 0);
    assert(state.retains == 1 && state.releases == 1);
}

static uint64_t store_file_row(wdb_database *db, uint64_t collection, uint64_t bits) {
    const uint8_t key[] = {'k'};
    wdb_value *value = NULL;
    wdb_tx *tx = NULL;
    uint64_t revision;
    assert(wdb_value_make(db, WDB_UNSIGNED, bits, NULL, 0, &value) == WDB_OK);
    assert(wdb_tx_begin(db, WDB_RECORDS, &tx) == WDB_OK);
    assert(wdb_tx_put(tx, collection, key, sizeof(key), value) == WDB_OK);
    assert(wdb_value_release(value) == WDB_OK);
    assert(wdb_tx_commit(tx, &revision) == WDB_OK);
    assert(wdb_tx_release(tx) == WDB_OK);
    return revision;
}

static void check_file_row(wdb_snapshot *view, uint64_t collection, uint64_t revision,
                           uint64_t expected) {
    const uint8_t key[] = {'k'};
    wdb_value *value = NULL;
    uint64_t actual;
    assert(wdb_snapshot_revision(view, &actual) == WDB_OK && actual == revision);
    assert(wdb_snapshot_get(view, collection, key, sizeof(key), &value) == WDB_OK);
    assert(wdb_value_scalar(value, &actual) == WDB_OK && actual == expected);
    assert(wdb_value_release(value) == WDB_OK);
}

static void check_file_roots(wdb_database *db, uint64_t collection, uint64_t retained) {
    wdb_snapshot *view = NULL;
    assert(wdb_snapshot_acquire(db, &view) == WDB_OK);
    check_file_row(view, collection, 4, 23);
    assert(wdb_snapshot_release(view) == WDB_OK);
    assert(wdb_retained_open(db, retained, &view) == WDB_OK);
    check_file_row(view, collection, 2, 17);
    assert(wdb_close(db) == WDB_BUSY);
    assert(wdb_snapshot_release(view) == WDB_OK);
}

static FILE *open_file_image(const char *path) {
#if defined(_MSC_VER)
    FILE *file = NULL;
    if (fopen_s(&file, path, "rb") != 0) return NULL;
    return file;
#else
    return fopen(path, "rb");
#endif
}

static uint8_t *read_file_image(const char *path, size_t *size) {
    FILE *file = open_file_image(path);
    assert(file != NULL && fseek(file, 0, SEEK_END) == 0);
    long length = ftell(file);
    assert(length > 0 && length <= 1024L * 1024L);
    assert(fseek(file, 0, SEEK_SET) == 0);
    *size = (size_t)length;
    uint8_t *bytes = (uint8_t *)malloc(*size);
    assert(bytes != NULL && fread(bytes, 1, *size, file) == *size);
    assert(fclose(file) == 0);
    return bytes;
}

enum create_refusal_state { NO_FILE, EMPTY_FILE, PUBLISHED_FILE };

static enum create_refusal_state check_refused_creation(const char *path) {
    FILE *file = open_file_image(path);
    if (file == NULL) {
        assert(errno == ENOENT);
        return NO_FILE;
    }
    assert(fseek(file, 0, SEEK_END) == 0);
    long length = ftell(file);
    assert(length >= 0 && fclose(file) == 0);
    wdb_database *db = (wdb_database *)(uintptr_t)1;
    wdb_status status = wdb_file_open((const uint8_t *)path, strlen(path), &db);
    enum create_refusal_state state = EMPTY_FILE;
    if (length == 0) {
        assert(status == WDB_CORRUPT && db == NULL);
    } else {
        assert(status == WDB_OK);
        wdb_snapshot *view = NULL;
        uint64_t revision;
        size_t required;
        assert(wdb_snapshot_acquire(db, &view) == WDB_OK);
        assert(wdb_snapshot_revision(view, &revision) == WDB_OK && revision == 0);
        assert(wdb_retained_ids(db, NULL, 0, &required) == WDB_OK && required == 0);
        assert(wdb_snapshot_release(view) == WDB_OK);
        assert(wdb_close(db) == WDB_OK);
        state = PUBLISHED_FILE;
    }
    assert(remove(path) == 0);
    return state;
}

static void verify_file_allocator_contract(void) {
    const char path[] = "allocator-roots.wdb";
    const char trial[] = "allocator-refusal.wdb";
    /* Standalone runs must also start with unused fixture paths. */
    assert(open_file_image(path) == NULL && errno == ENOENT);
    assert(open_file_image(trial) == NULL && errno == ENOENT);
    check_file_factory_rejection(wdb_file_create_with_options, path);
    check_file_factory_rejection(wdb_file_open_with_options, path);

    allocator_state creating = {0, 0, 0, 0, 0, 0};
    wdb_database *db = open_allocated_file(wdb_file_create_with_options, path, &creating);
    size_t create_requests = creating.requests;
    uint64_t collection, retained;
    assert(wdb_collection_create(db, &collection) == WDB_OK);
    assert(store_file_row(db, collection, 17) == 2);
    wdb_snapshot *snapshot = NULL;
    assert(wdb_snapshot_acquire(db, &snapshot) == WDB_OK);
    assert(wdb_snapshot_retain(db, snapshot, &retained) == WDB_OK);
    assert(wdb_snapshot_release(snapshot) == WDB_OK);
    assert(store_file_row(db, collection, 23) == 4);
    assert(wdb_close(db) == WDB_OK && creating.live == 0 && creating.releases == 1);

    size_t create_states[3] = {0, 0, 0};
    for (size_t request = 1; request <= create_requests; request++) {
        file_factory_refusal(wdb_file_create_with_options, trial, request);
        enum create_refusal_state state = check_refused_creation(trial);
        if (state == PUBLISHED_FILE) assert(request == create_requests);
        create_states[state]++;
    }
    assert(create_states[NO_FILE] > 0 && create_states[EMPTY_FILE] > 0);
    assert(create_states[PUBLISHED_FILE] == 1);
    assert(wdb_file_create((const uint8_t *)trial, strlen(trial), &db) == WDB_OK);
    assert(wdb_close(db) == WDB_OK && remove(trial) == 0);

    allocator_state reopening = {0, 0, 0, 0, 0, 0};
    db = open_allocated_file(wdb_file_open_with_options, path, &reopening);
    size_t open_requests = reopening.requests;
    check_file_roots(db, collection, retained);
    assert(wdb_close(db) == WDB_OK && reopening.live == 0 && reopening.releases == 1);
    size_t image_size;
    uint8_t *image = read_file_image(path, &image_size);
    for (size_t request = 1; request <= open_requests; request++) {
        file_factory_refusal(wdb_file_open_with_options, path, request);
        size_t actual_size;
        uint8_t *actual = read_file_image(path, &actual_size);
        assert(actual_size == image_size && memcmp(actual, image, image_size) == 0);
        free(actual);
        assert(wdb_file_open((const uint8_t *)path, strlen(path), &db) == WDB_OK);
        check_file_roots(db, collection, retained);
        assert(wdb_close(db) == WDB_OK);
    }
    free(image);
    assert(remove(path) == 0);
    printf("C file allocator contract: %zu create refusals (%zu absent, %zu empty, %zu published), "
           "%zu reopen refusals preserved exact bytes and both roots.\n",
           create_requests, create_states[NO_FILE], create_states[EMPTY_FILE],
           create_states[PUBLISHED_FILE], open_requests);
}
