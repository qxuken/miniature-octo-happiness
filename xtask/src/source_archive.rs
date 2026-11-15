//! Bounded Git archive preparation shared by compatibility runners.
use crate::support::{cmd, path, sha};
use anyhow::{Result, ensure};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub fn revision(
    output: &Path,
    reference: &str,
    label: Option<&str>,
    mut command: impl FnMut(Vec<OsString>, &str, f64) -> Result<String>,
) -> Result<(String, PathBuf, String)> {
    let prefix = label.unwrap_or("engine");
    ensure!(
        !prefix.is_empty()
            && prefix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "invalid archive label"
    );
    let revision = command(
        vec![
            "git".into(),
            "rev-parse".into(),
            "--verify".into(),
            "--end-of-options".into(),
            format!("{reference}^{{commit}}").into(),
        ],
        &format!("{prefix}-revision"),
        30.,
    )?;
    ensure!(
        revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid engine revision"
    );
    let files = command(
        cmd(["git", "ls-tree", "-r", &revision]),
        &format!("{prefix}-files"),
        30.,
    )?;
    ensure!(
        !files.is_empty()
            && files
                .lines()
                .all(|l| l.starts_with("100644 blob ") || l.starts_with("100755 blob ")),
        "nonregular engine archive input"
    );
    let (tar, source) = if let Some(label) = label {
        let parent = output.join("source");
        fs::create_dir_all(&parent)?;
        (output.join(format!("{label}.tar")), parent.join(label))
    } else {
        (output.join("source.tar"), output.join("source"))
    };
    fs::create_dir(&source)?;
    command(
        vec![
            "git".into(),
            "archive".into(),
            "--format=tar".into(),
            "--output".into(),
            path(&tar),
            revision.clone().into(),
        ],
        &format!("{prefix}-archive"),
        30.,
    )?;
    command(
        vec![
            "tar".into(),
            "-xf".into(),
            path(&tar),
            "-C".into(),
            path(&source),
        ],
        &if label.is_none() {
            "extract-source".to_owned()
        } else {
            format!("{prefix}-extract")
        },
        30.,
    )?;
    Ok((revision, source, sha(&tar)?))
}
