use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::Command,
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;

pub fn space(path: &Path) -> (u64, u64) {
    let output = Command::new("stat")
        .args(["-f", "-c", "%T %S %b %a", "--"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let fields: Vec<_> = text.split_ascii_whitespace().collect();
    let ["tmpfs", block, blocks, available] = fields.as_slice() else {
        panic!("expected a private tmpfs: {text}");
    };
    let block: u64 = block.parse().unwrap();
    let capacity = block.checked_mul(blocks.parse().unwrap()).unwrap();
    let available = block.checked_mul(available.parse().unwrap()).unwrap();
    assert!(block > 0 && capacity <= MAX_BYTES && available <= capacity);
    (block, available)
}

pub fn empty_mount(variable: &str) -> (PathBuf, u64) {
    let mount = PathBuf::from(std::env::var_os(variable).unwrap());
    let mount_type = Command::new("findmnt")
        .args(["-n", "-o", "FSTYPE", "--mountpoint"])
        .arg(&mount)
        .output()
        .unwrap();
    assert!(mount_type.status.success(), "{mount_type:?}");
    assert_eq!(mount_type.stdout, b"tmpfs\n");
    let (_, available) = space(&mount);
    assert!(available >= 1024 * 1024);
    assert!(fs::read_dir(&mount).unwrap().next().is_none());
    (mount, available)
}

pub fn fill(path: &Path, room: u64) -> PathBuf {
    let (block, available) = space(path);
    assert!(room < available && room.is_multiple_of(block));
    let ballast = path.join("ballast");
    fs::File::create_new(&ballast)
        .unwrap()
        .write_all(&vec![0x6a; (available - room) as usize])
        .unwrap();
    assert_eq!(space(path).1, room);
    ballast
}

pub fn assert_full(path: &Path) {
    let (block, available) = space(path);
    assert_eq!(available, 0);
    let probe = path.join("space-probe");
    let mut file = fs::File::create_new(&probe).unwrap();
    let error = file.write_all(&vec![0; block as usize]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::StorageFull);
    assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
    drop(file);
    fs::remove_file(probe).unwrap();
}
