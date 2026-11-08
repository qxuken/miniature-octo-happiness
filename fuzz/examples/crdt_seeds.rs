use std::{fs, io, path::Path};
fn main() -> io::Result<()> {
    for (target, seeds) in [
        (
            "crdt_frames",
            &[
                ("valid-peer", &[1][..]),
                ("same-actor", &[1, 64, 1]),
                ("forged-local-progress", &[1, 65, 6]),
                ("policy-generation", &[1, 24, 3]),
                ("truncated", &[0, 1, 2, 3]),
            ][..],
        ),
        (
            "crdt_schedule",
            &[
                (
                    "offline-counter-set",
                    &[0, 10, 1, 20, 2, 3, 3, 4, 4, 0, 5, 0][..],
                ),
                (
                    "delay-loss-restart",
                    &[0, 10, 6, 0, 1, 20, 0, 7, 4, 1, 8, 0, 10, 0, 5, 0, 11, 0],
                ),
            ][..],
        ),
    ] {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("seeds")
            .join(target);
        fs::create_dir_all(&directory)?;
        for (name, bytes) in seeds {
            fs::write(directory.join(name), bytes)?;
        }
    }
    Ok(())
}
