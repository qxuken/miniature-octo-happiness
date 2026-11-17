#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|input: &[u8]| weekend_db_fuzz::schema_index_migration(input));
