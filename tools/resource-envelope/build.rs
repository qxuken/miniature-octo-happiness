use std::{fs, path::Path};

fn main() {
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/engine.rs");
    println!("cargo:rerun-if-changed={}", engine.display());
    println!("cargo:rustc-check-cfg=cfg(legacy_allocation_hook)");
    // Resource campaigns overlay this driver on the archived engine. Preserve
    // the original injection boundary when replaying an older source.
    let source = fs::read_to_string(engine).expect("read archived engine source");
    if source.contains("pub fn fail_allocations_after(") {
        println!("cargo:rustc-cfg=legacy_allocation_hook");
    }
}
