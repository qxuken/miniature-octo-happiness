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
    for (name, bytes) in [
        (
            "nested-prefix-keys",
            vec![48, 128, 7, 48, 129, 8, 2, 128, 0, 1, 129, 0],
        ),
        (
            "float-bits",
            vec![35, 0, 0, 35, 1, 1, 35, 2, 2, 35, 3, 3, 35, 4, 4],
        ),
        (
            "list-boundaries",
            vec![4, 32, 0, 6, 31, 0, 5, 32, 9, 7, 255, 0, 4, 255, 0],
        ),
    ] {
        seed("value_ops", name, &bytes)?;
    }
    let history = [
        2, 0, 0, 0, 0, 7, 2, 0, 0, 3, 0, 1, 5, 0, 0, 6, 0, 0, 4, 0, 0, 1, 1, 0,
    ];
    for mode in 0..5 {
        let mut input = vec![mode];
        input.extend_from_slice(&history);
        seed("image_and_ops", &format!("history-mode-{mode}"), &input)?;
    }
    for at in 0..24 {
        let mut input = vec![at, at % 4, at % 2];
        input.extend_from_slice(&history);
        seed("retention_compact_crash", &format!("copy-io-{at}"), &input)?;
    }
    for (name, commands) in [
        (
            "clone-and-buffers",
            vec![0, 24, 9, 1, 0, 1, 2, 1, 0, 2, 1, 2, 2, 1, 3, 4, 0, 0],
        ),
        (
            "foreign-map",
            vec![
                0, 16, 7, 128, 0, 42, 3, 0, 42, 130, 17, 8, 3, 1, 43, 128, 1, 43,
            ],
        ),
        (
            "empty-and-self-clone",
            vec![0, 0, 0, 1, 0, 0, 2, 0, 0, 3, 0, 0, 4, 0, 0],
        ),
    ] {
        seed("ffi_ops", name, &commands)?;
    }
    Ok(())
}
