use std::{fs, io, path::Path};
use weekend_db::storage::REGION;
use weekend_db_fuzz::fixture_image;

fn seed(target: &str, name: &str, bytes: &[u8]) -> io::Result<()> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("seeds")
        .join(target);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(name), bytes)
}

fn main() -> io::Result<()> {
    let image = fixture_image();
    seed("open_image_bytes", "retained-v2.wdb", image)?;
    seed("open_image_bytes", "truncated-header", &image[..REGION])?;
    for mode in 0..10 {
        seed("open_structured_image", &format!("mode-{mode}"), &[mode, 0])?;
    }
    seed("open_structured_image", "lost-witness-and-slot", &[1, 1])?;
    seed(
        "open_structured_image",
        "lost-witness-and-torn-slot",
        &[2, 1],
    )?;
    seed(
        "open_structured_image",
        "patched-regions",
        &[10, 0, 0, 0, 1, 1, 5, 8, 0, 1, 1],
    )?;
    for (name, at, mode, power, arg, flags) in [
        ("append-drop", 0, 1, 0, 0, 0),
        ("root-drop", 1, 1, 0, 0, 0),
        ("root-persist", 1, 3, 1, 0, 1),
        ("root-masked", 1, 0, 2, 1, 1),
        ("root-torn", 1, 0, 3, 1, 2),
        ("witness-drop", 2, 1, 0, 0, 2),
        ("short-append", 4, 2, 0, 1, 0),
        ("tear-append", 4, 4, 1, 3, 0),
    ] {
        seed(
            "commit_crash",
            name,
            &[at, mode, power, arg, flags, 0, 7, 0x81, 0, 4, 9],
        )?;
    }
    for (name, commands) in [
        (
            "independent-edits",
            [
                [0x00, 0, 0],
                [0x01, 0, 0],
                [0x20, 0, 20],
                [0x21, 1, 21],
                [0x50, 0, 0],
                [0x51, 0, 0],
            ],
        ),
        (
            "present-delete-conflict",
            [
                [0x00, 0, 0],
                [0x10, 0, 0],
                [0x01, 0, 0],
                [0x31, 0, 0],
                [0x51, 0, 0],
                [0x50, 0, 0],
            ],
        ),
        (
            "absence-noop-conflict",
            [
                [0x00, 0, 0],
                [0x10, 3, 0],
                [0x01, 0, 0],
                [0x31, 2, 0],
                [0x51, 0, 0],
                [0x50, 0, 0],
            ],
        ),
        (
            "empty-scan-drop",
            [
                [0x00, 0, 0],
                [0x44, 0, 0],
                [0x64, 0, 0],
                [0x50, 0, 0],
                [0x00, 0, 0],
                [0x14, 0, 0],
            ],
        ),
        (
            "whole-root-empty-commit",
            [
                [0x08, 0, 0],
                [0x01, 0, 0],
                [0x51, 0, 0],
                [0x50, 0, 0],
                [0x00, 0, 0],
                [0x70, 0, 0],
            ],
        ),
        (
            "own-writes-scan",
            [
                [0x00, 0, 0],
                [0x20, 2, 20],
                [0x10, 2, 0],
                [0x30, 0, 0],
                [0x40, 0, 0],
                [0x50, 0, 0],
            ],
        ),
    ] {
        seed("txn_schedule", name, commands.as_flattened())?;
    }
    Ok(())
}
