use std::{fs, io, path::Path};
fn seed(target: &str, name: &str, bytes: &[u8]) -> io::Result<()> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("seeds")
        .join(target);
    fs::create_dir_all(&directory)?;
    fs::write(directory.join(name), bytes)
}
fn main() -> io::Result<()> {
    for tag in 0..=11 {
        seed("snapshot_frames", &format!("tag-{tag}"), &[tag])?;
    }
    for revision in 0u64..=3 {
        let mut bytes = vec![2];
        bytes.extend_from_slice(&[42; 16]);
        bytes.extend_from_slice(&revision.to_le_bytes());
        bytes.extend_from_slice(&[3; 32]);
        seed("snapshot_frames", &format!("revision-{revision}"), &bytes)?;
    }
    for mode in 0..8 {
        seed(
            "snapshot_install_schedule",
            &format!("install-mode-{mode}"),
            &[0, 0, mode * 2 + 1, 7, 1, 0, 10, 0],
        )?;
    }
    for malformed in 0..4 {
        seed(
            "snapshot_install_schedule",
            &format!("malformed-{malformed}"),
            &[15, malformed, 1, 0],
        )?;
    }
    seed(
        "snapshot_install_schedule",
        "first-interrupted-cleanup",
        &[2, 0, 12, 0, 0, 0],
    )?;
    for (name, bytes) in [
        (
            "interrupted-upload-duplicate-upgrade",
            &[0, 0, 243, 243, 8, 1, 15, 0, 1, 1, 10, 0][..],
        ),
        ("ready-flush-first", &[6, 128, 10, 0, 0, 0, 10, 0][..]),
        ("terminal-flush-first", &[8, 128, 10, 0, 0, 0, 10, 0][..]),
        (
            "ready-flush-upgrade",
            &[0, 0, 7, 128, 10, 0, 1, 0, 10, 0][..],
        ),
        (
            "terminal-flush-upgrade",
            &[0, 0, 9, 128, 10, 0, 1, 0, 10, 0][..],
        ),
        (
            "terminal-flush-duplicates",
            &[0, 0, 8, 128, 10, 0, 1, 0, 9, 128, 10, 0][..],
        ),
        (
            "first-flush-stale-and-duplicate",
            &[1, 0, 6, 128, 10, 0, 7, 128, 10, 0][..],
        ),
    ] {
        seed("snapshot_install_schedule", name, bytes)?;
    }
    Ok(())
}
