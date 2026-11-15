//! Independent CLI receipt parsing and evidence credential cleanup.
use crate::support::qualification_inventory as inventory;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;
use std::{fs, path::Path};

pub(crate) const RECEIPT: &str = "follower durable_revision=";
pub(crate) fn hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Receipt {
    pub revision: u64,
    pub stream: String,
    pub digest: String,
}
pub(crate) fn receipt(output: &str, expected_stream: &str) -> Result<Receipt> {
    let mut receipts = output.lines().filter_map(|line| line.strip_prefix(RECEIPT));
    let row = receipts.next().context("durable receipt missing")?;
    ensure!(receipts.next().is_none(), "duplicate durable receipt");
    let (revision, rest) = row
        .split_once(" stream=")
        .context("missing receipt stream")?;
    let (stream, digest) = rest
        .split_once(" digest=")
        .context("missing receipt digest")?;
    ensure!(
        hex(stream, 32) && stream == expected_stream && hex(digest, 64),
        "invalid durable stream or digest"
    );
    ensure!(
        !revision.is_empty()
            && revision.len() <= 20
            && revision.bytes().all(|c| c.is_ascii_digit())
            && (revision == "0" || !revision.starts_with('0')),
        "invalid durable revision"
    );
    Ok(Receipt {
        revision: revision.parse()?,
        stream: stream.into(),
        digest: digest.into(),
    })
}

pub(crate) fn description_identity(text: &str) -> Result<String> {
    let mut values = text
        .split_ascii_whitespace()
        .filter_map(|part| part.strip_prefix("file_identity="));
    let identity = values.next().context("CLI file identity missing")?;
    ensure!(
        hex(identity, 32) && values.next().is_none(),
        "invalid or duplicate CLI file identity"
    );
    Ok(identity.to_owned())
}

pub(crate) fn selected_slot(status: &str) -> Result<&str> {
    let mut fields = status
        .split_ascii_whitespace()
        .filter_map(|part| part.strip_prefix("selected_slot="));
    let slot = match fields.next() {
        Some("Some(0)") => "0",
        Some("Some(1)") => "1",
        _ => anyhow::bail!("selected follower slot missing"),
    };
    ensure!(fields.next().is_none(), "duplicate selected follower slot");
    Ok(slot)
}

pub(crate) fn scrub_keys(directories: &[&Path]) -> Result<()> {
    let mut failures = Vec::new();
    for directory in directories {
        let path = directory.join("key.der");
        if let Err(error) = fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            failures.push(format!("{}: {error}", path.display()));
        }
    }
    ensure!(
        failures.is_empty(),
        "private key cleanup failed: {}",
        failures.join(", ")
    );
    Ok(())
}

pub(crate) fn finish_with_key_cleanup(
    evidence: &mut serde_json::Value,
    output: &Path,
    result: Result<()>,
    directories: &[&Path],
) -> Result<()> {
    let scrubbed = scrub_keys(directories);
    evidence["private_credentials_retained"] = json!(scrubbed.is_err());
    let result = match (result, scrubbed) {
        (Ok(()), Ok(())) => {
            let artifacts = inventory(output, &["source", "build", "evidence.json"]);
            artifacts.map(|files| evidence["artifacts"] = json!(files))
        }
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(work), Err(cleanup)) => Err(anyhow::anyhow!("{work:#}\n{cleanup:#}")),
    };
    evidence["verified"] = json!(result.is_ok());
    result
}
