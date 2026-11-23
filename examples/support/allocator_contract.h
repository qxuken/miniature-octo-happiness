/* Used by the packaged C11 and C++17 consumers. */
#ifdef _WIN32
#include <windows.h>
#endif

typedef struct {
    volatile long lock;
    size_t requests, fail_at, live, retains, releases;
} allocator_state;

typedef struct {
    void *base;
    size_t size, align;
} allocation_layout;

static void lock_allocator(allocator_state *state) {
#ifdef _WIN32
    while (InterlockedExchange(&state->lock, 1) != 0) {}
#else
    while (__atomic_exchange_n(&state->lock, 1, __ATOMIC_ACQUIRE) != 0) {}
#endif
}

static void unlock_allocator(allocator_state *state) {
#ifdef _WIN32
    InterlockedExchange(&state->lock, 0);
#else
    __atomic_store_n(&state->lock, 0, __ATOMIC_RELEASE);
#endif
}

static void *contract_allocate(void *context, size_t size, size_t align) {
    allocator_state *state = (allocator_state *)context;
    assert(size != 0 && align != 0 && (align & (align - 1)) == 0);
    lock_allocator(state);
    assert(state->retains == 1 && state->releases == 0);
    state->requests++;
    if (state->requests == state->fail_at ||
        align - 1 > SIZE_MAX - sizeof(allocation_layout) ||
        size > SIZE_MAX - sizeof(allocation_layout) - (align - 1)) {
        unlock_allocator(state);
        return NULL;
    }
    void *base = malloc(size + (align - 1) + sizeof(allocation_layout));
    if (base == NULL) {
        unlock_allocator(state);
        return NULL;
    }
    uint8_t *start = (uint8_t *)base + sizeof(allocation_layout);
    size_t padding = (size_t)((0 - (uintptr_t)start) & (align - 1));
    uint8_t *block = start + padding;
    allocation_layout layout = {base, size, align};
    memcpy(block - sizeof(layout), &layout, sizeof(layout));
    state->live++;
    unlock_allocator(state);
    return block;
}

static void contract_deallocate(void *context, void *block, size_t size, size_t align) {
    allocator_state *state = (allocator_state *)context;
    allocation_layout layout;
    memcpy(&layout, (uint8_t *)block - sizeof(layout), sizeof(layout));
    assert(size == layout.size && align == layout.align);
    lock_allocator(state);
    assert(state->retains == 1 && state->releases == 0 && state->live != 0);
    free(layout.base);
    state->live--;
    unlock_allocator(state);
}

static void contract_retain(void *context) {
    allocator_state *state = (allocator_state *)context;
    lock_allocator(state);
    assert(state->retains == 0 && state->releases == 0 && state->requests == 0);
    state->retains++;
    unlock_allocator(state);
}

static void contract_release(void *context) {
    allocator_state *state = (allocator_state *)context;
    lock_allocator(state);
    assert(state->retains == 1 && state->releases == 0 && state->live == 0);
    state->releases++;
    unlock_allocator(state);
}

static wdb_allocator contract_callbacks(allocator_state *state) {
    wdb_allocator callbacks = {
        state, contract_allocate, contract_deallocate, contract_retain, contract_release
    };
    return callbacks;
}

static void check_refused_options(wdb_database_options *options, allocator_state *state) {
    wdb_database *db = (wdb_database *)(uintptr_t)1;
    assert(wdb_open_with_options(options, &db) == WDB_INVALID && db == NULL);
    assert(state->requests == 0 && state->retains == 0 && state->releases == 0);
}

static void verify_allocator_contract(void) {
    allocator_state state = {0, 0, 0, 0, 0, 0};
    wdb_allocator callbacks = contract_callbacks(&state);
    wdb_database_options options;
    assert(wdb_database_options_init(NULL) == WDB_INVALID);
    assert(wdb_database_options_init(&options) == WDB_OK && options.allocator == NULL);
    size_t depth = options.limits.max_depth;
    options.allocator = &callbacks;
    options.limits.max_depth = 0;
    check_refused_options(&options, &state);
    options.limits.max_depth = depth;
    callbacks.release_context = NULL;
    check_refused_options(&options, &state);
    callbacks = contract_callbacks(&state);
    options.limits.max_bytes = 3;
    options.limits.max_live_bytes = 1024u * 1024u;

    wdb_database *db = NULL;
    assert(wdb_open_with_options(&options, &db) == WDB_OK);
    size_t open_requests = state.requests;
    assert(open_requests > 0 && state.retains == 1 && state.releases == 0);
    /* Open copies both descriptors. */
    callbacks.allocate = NULL;
    callbacks.deallocate = NULL;
    callbacks.release_context = NULL;
    options.limits.max_bytes = 0;

    wdb_limits limits;
    wdb_memory_usage usage;
    assert(wdb_database_get_limits(db, NULL) == WDB_INVALID);
    assert(wdb_database_get_memory_usage(db, NULL) == WDB_INVALID);
    assert(wdb_database_get_limits(db, &limits) == WDB_OK);
    assert(limits.max_bytes == 3 && limits.max_depth == depth);
    assert(wdb_database_get_memory_usage(db, &usage) == WDB_OK);
    assert(usage.limit_bytes == options.limits.max_live_bytes);
    assert(usage.live_bytes <= usage.peak_bytes && usage.peak_bytes <= usage.limit_bytes);
    uint64_t collection, id, revision;
    wdb_tx *tx = NULL;
    wdb_value *value = (wdb_value *)(uintptr_t)1;
    const uint8_t bytes[] = {'a', 0, 'b', 'c'};
    assert(wdb_value_make(db, WDB_BYTES, 0, bytes, sizeof(bytes), &value) == WDB_LIMIT && value == NULL);
    assert(wdb_value_make(db, WDB_BYTES, 0, bytes, 3, &value) == WDB_OK);
    assert(wdb_collection_create(db, &collection) == WDB_OK);
    assert(wdb_tx_begin(db, WDB_RECORDS, &tx) == WDB_OK);
    assert(wdb_tx_put(tx, collection, bytes, 1, value) == WDB_OK);
    assert(wdb_value_release(value) == WDB_OK);
    assert(wdb_tx_commit(tx, &revision) == WDB_OK);
    assert(wdb_tx_release(tx) == WDB_OK);

    wdb_snapshot *view = (wdb_snapshot *)(uintptr_t)1;
    size_t before = state.live;
    state.fail_at = state.requests + 1;
    assert(wdb_snapshot_acquire(db, &view) == WDB_OUT_OF_MEMORY && view == NULL);
    assert(state.requests == state.fail_at && state.live == before && state.releases == 0);
    assert(wdb_snapshot_acquire(db, &view) == WDB_OK);
    assert(wdb_snapshot_retain(db, view, &id) == WDB_OK);
    assert(wdb_snapshot_release(view) == WDB_OK);
    assert(wdb_retained_open(db, id, &view) == WDB_OK);
    assert(wdb_retained_release(db, id) == WDB_OK);
    assert(wdb_close(db) == WDB_BUSY && state.releases == 0);
    wdb_scan *scan = NULL;
    assert(wdb_snapshot_scan(view, collection, &scan) == WDB_OK);
    assert(wdb_snapshot_release(view) == WDB_OK);
    assert(wdb_close(db) == WDB_BUSY && state.releases == 0);
    assert(wdb_scan_value(scan, 0, &value) == WDB_OK);
    uint8_t read[3];
    size_t required;
    assert(wdb_value_bytes(value, read, sizeof(read), &required) == WDB_OK);
    assert(required == sizeof(read) && memcmp(read, bytes, sizeof(read)) == 0);
    assert(wdb_scan_release(scan) == WDB_OK);
    assert(wdb_value_release(value) == WDB_OK);
    assert(wdb_close(db) == WDB_OK && state.releases == 1 && state.live == 0);

    options.limits.max_bytes = 3;
    for (size_t fail_at = 1; fail_at <= open_requests; fail_at++) {
        allocator_state refusing = {0, 0, fail_at, 0, 0, 0};
        callbacks = contract_callbacks(&refusing);
        db = (wdb_database *)(uintptr_t)1;
        assert(wdb_open_with_options(&options, &db) == WDB_OUT_OF_MEMORY && db == NULL);
        assert(refusing.requests == fail_at && refusing.live == 0);
        assert(refusing.retains == 1 && refusing.releases == 1);
    }
    printf("C allocator contract: %zu open refusals and snapshot refusal, retry and close passed.\n",
           open_requests);
}
