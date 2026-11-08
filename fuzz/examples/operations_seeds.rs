use std::{fs, io, path::Path};
fn main() -> io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("seeds");
    for target in [
        "logical_import_export",
        "backup_restore",
        "bytestore_contract",
    ] {
        fs::create_dir_all(root.join(target))?;
    }
    for mode in 0..5 {
        fs::write(
            root.join("logical_import_export")
                .join(format!("mode-{mode}")),
            [mode, 0, 0, 1],
        )?;
    }
    fs::write(
        root.join("logical_import_export")
            .join("repaired-zero-next-id"),
        [4, 48, 0, 2],
    )?;
    for step in 0..24 {
        fs::write(
            root.join("backup_restore").join(format!("step-{step}")),
            [step, step % 3, step % 2, step],
        )?;
    }
    for r in 0..6 {
        for w in 0..6 {
            fs::write(
                root.join("bytestore_contract")
                    .join(format!("progress-{r}-{w}")),
                [r, w],
            )?;
        }
    }
    Ok(())
}
