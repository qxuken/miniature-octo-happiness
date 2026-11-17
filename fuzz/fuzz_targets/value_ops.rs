#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|input: &[u8]| weekend_db_fuzz::value_ops(input));
