//! Fresh-process timing; on Linux, count resident file pages before each cold sample.
use super::*;
use std::process::Command;

#[cfg(target_os = "linux")]
fn evict_owned_fixture(path: &Path) -> usize {
    use std::{ffi::c_void, os::fd::AsRawFd};
    unsafe extern "C" {
        fn posix_fadvise(fd: i32, offset: i64, len: i64, advice: i32) -> i32;
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> i32;
        fn mincore(addr: *mut c_void, len: usize, vec: *mut u8) -> i32;
        fn getpagesize() -> i32;
    }
    let file = fs::File::open(path).unwrap();
    file.sync_all().unwrap();
    let fd = file.as_raw_fd();
    let len = file.metadata().unwrap().len() as usize;
    // SAFETY: the file remains open for every call. mincore writes one byte per
    // system page in a live file-backed mapping; nobody dereferences the mapping.
    unsafe {
        assert_eq!(posix_fadvise(fd, 0, 0, 4), 0);
        let map = mmap(std::ptr::null_mut(), len, 0, 1, fd, 0);
        assert_ne!(map as isize, -1);
        let mut pages = vec![0; len.div_ceil(getpagesize() as usize)];
        assert_eq!(mincore(map, len, pages.as_mut_ptr()), 0);
        assert_eq!(munmap(map, len), 0);
        pages.into_iter().filter(|p| p & 1 == 1).count()
    }
}

#[test]
#[ignore = "fresh-process packaged reader benchmark"]
fn packaged_reader() {
    const PATH: &str = "WDB_CAPACITY_BENCH_FILE";
    if let Some(path) = std::env::var_os(PATH) {
        let now = Instant::now();
        let v = ReadOnlyDatabase::open_file(path, limits().into()).unwrap();
        let open = now.elapsed();
        let t = Instant::now();
        let value = v
            .snapshot()
            .get(v.collection_id(1), &100u32.to_be_bytes())
            .unwrap()
            .unwrap();
        let point = t.elapsed();
        assert_eq!(value.as_bytes().unwrap()[..4], 100u32.to_le_bytes());
        drop(value);
        let t = Instant::now();
        v.check().unwrap();
        let full = t.elapsed();
        println!(
            "SAMPLE {} {} {} {}",
            open.as_micros(),
            point.as_micros(),
            full.as_micros(),
            v.memory_usage().peak_bytes
        );
        return;
    }
    let dir = Dir::new();
    let p = dir.path("benchmark");
    let db = create(&p, FileCapacity::GiB1);
    let c = db.collection_id(1);
    let mut bytes = vec![57; 1 << 20];
    for group in 0..240u32 {
        let mut tx = db.begin();
        for k in group * 4..group * 4 + 4 {
            bytes[..4].copy_from_slice(&k.to_le_bytes());
            tx.put(c, &k.to_be_bytes(), &db.bytes(&bytes).unwrap())
                .unwrap();
        }
        tx.commit().unwrap();
    }
    db.retain_snapshot(&db.snapshot()).unwrap();
    put(&db, 600, b"edited");
    drop(db);
    for (mode, repeats) in [("warm", 50usize), ("cold", 5usize)] {
        if mode == "cold" && !cfg!(target_os = "linux") {
            continue;
        }
        let mut results = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        #[cfg(target_os = "linux")]
        let mut max_resident = 0usize;
        #[cfg(not(target_os = "linux"))]
        let max_resident = 0usize;
        for _ in 0..repeats {
            if mode == "warm" {
                std::io::copy(&mut fs::File::open(&p).unwrap(), &mut std::io::sink()).unwrap();
            }
            #[cfg(target_os = "linux")]
            if mode == "cold" {
                let resident = evict_owned_fixture(&p);
                // Kernels can retain pages while a readahead request drains.
                assert!(resident < 256, "{resident} resident pages before sample");
                max_resident = max_resident.max(resident);
            }
            let out = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "capacity_benchmark::packaged_reader",
                    "--nocapture",
                ])
                .env(PATH, &p)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let stdout = String::from_utf8(out.stdout).unwrap();
            let sample = stdout
                .lines()
                .find_map(|l| l.strip_prefix("SAMPLE "))
                .unwrap();
            let vals: Vec<u64> = sample
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            assert_eq!(vals.len(), 4);
            for (col, v) in results.iter_mut().zip(vals) {
                col.push(v);
            }
        }
        for column in &mut results {
            column.sort_unstable();
        }
        let percentiles = results.map(|col| {
            (
                col[(repeats - 1) / 2],
                col[(repeats * 95).div_ceil(100) - 1],
            )
        });
        println!(
            "capacity-benchmark mode={mode} processes={repeats} file_bytes={} max_cold_resident_pages={max_resident} median/p95 (open_us,point_us,full_us,peak_bytes)={percentiles:?}",
            fs::metadata(&p).unwrap().len()
        );
    }
}
